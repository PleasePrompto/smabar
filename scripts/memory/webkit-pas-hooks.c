#define _GNU_SOURCE
#include <frida-gum.h>
#if !defined(__linux__) || !defined(__x86_64__)
#error "PAS hooks support only Linux x86_64 System V ABI"
#endif
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <limits.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* Diagnostic ABI: installed x86_64 WebKitGTK 2.52.6 and heaptrack 1.5.0. */
enum Kind { ALLOC, CALLOC, ALIGNED, REALLOC, STRDUP, MEMDUP, ISO, FREE };
struct Hook {
    const char *symbol;
    const char *name;
    enum Kind kind;
    bool sret;
    _Atomic uint64_t calls;
};
#define HOOK(symbol, name, kind, sret) {symbol, name, kind, sret, 0},
static struct Hook hooks[] = {
#include "pas-hooks.def"
};
#undef HOOK

struct Invocation {
    bool observed;
    bool outer;
    void **result_storage;
    void *old_pointer;
    GumReturnAddressArray trace;
};

static _Thread_local unsigned depth;
static _Thread_local bool reporting;
static _Thread_local const GumReturnAddressArray *reported_trace;
static GumInterceptor *interceptor;
static GumInvocationListener *listener;
static GumBacktracer *backtracer;
static void (*report_alloc)(void *, size_t);
static void (*report_free)(void *);
static void (*report_realloc)(void *, size_t, void *);
static void (*stop_heaptrack)(void);
static size_t (*allocation_size)(const void *);
static int (*original_unwind)(void **, int);
static void *unwind_address;
static char stats_path[PATH_MAX];
static int stats_fd = -1;
static bool collector_initialized;
static bool started;
static _Atomic uint64_t allocations, frees, failures, nested;

static void fail(const char *operation, const char *detail)
{
    fprintf(stderr, "PAS diagnostic %s failed: %s\n", operation, detail);
    _exit(127);
}

static void *required_symbol(void *module, const char *name)
{
    dlerror();
    void *address = dlsym(module, name);
    const char *error = dlerror();
    if (error || !address)
        fail(name, error ? error : "missing symbol");
    return address;
}

static int reserve_output(const char *path)
{
    int file = open(path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
    if (file < 0)
        fail("reserve fresh output", strerror(errno));
    return file;
}

/* libheaptrack.h 1.5.0: the callback receives LineWriter&. On the supported
 * SysV ABI this is an opaque pointer; it runs only after successful open/lock. */
static void collector_ready(void *writer)
{
    (void)writer;
    collector_initialized = true;
}

/* Heaptrack's public API obtains its trace through libunwind's public API.
 * Preserve normal unwinding; for our annotations supply Gum's original caller
 * stack, avoiding trampoline frames. Trace::fill skips two frames in 1.5.0. */
static int annotation_unwind(void **addresses, int capacity)
{
    if (!reported_trace)
        return original_unwind(addresses, capacity);
    if (capacity < 2)
        return 0;
    unsigned length = reported_trace->len;
    if (length > (unsigned)capacity - 2)
        length = (unsigned)capacity - 2;
    addresses[0] = addresses[1] = (void *)annotation_unwind;
    memcpy(addresses + 2, reported_trace->items, length * sizeof(void *));
    return (int)length + 2;
}

static void on_enter(GumInvocationContext *context, gpointer unused)
{
    (void)unused;
    struct Invocation *call = gum_invocation_context_get_listener_invocation_data(context, sizeof(*call));
    memset(call, 0, sizeof(*call));
    if (reporting)
        return;
    call->observed = true;
    call->outer = depth++ == 0;
    if (!call->outer) {
        atomic_fetch_add(&nested, 1);
        return;
    }
    struct Hook *hook = gum_invocation_context_get_listener_function_data(context);
    atomic_fetch_add(&hook->calls, 1);
    reporting = true;
    if (hook->kind == FREE) {
        void *pointer = gum_invocation_context_get_nth_argument(context, 0);
        if (pointer) {
            report_free(pointer);
            atomic_fetch_add(&frees, 1);
        }
    } else {
        if (hook->sret)
            call->result_storage = gum_invocation_context_get_nth_argument(context, 0);
        if (hook->kind == REALLOC)
            call->old_pointer = gum_invocation_context_get_nth_argument(context, hook->sret ? 1 : 0);
        /* Unwind from the caller's post-call state, outside the patched callee
         * prologue. Gum reserves frame zero for *(rsp), so replace it with the
         * original return PC after walking the caller's DWARF frame. */
        GumCpuContext caller = *context->cpu_context;
        gpointer return_address = gum_invocation_context_get_return_address(context);
        caller.rip = (uintptr_t)return_address;
        caller.rsp += sizeof(void *);
        gum_backtracer_generate(backtracer, &caller, &call->trace);
        call->trace.items[0] = return_address;
    }
    reporting = false;
}

static void on_leave(GumInvocationContext *context, gpointer unused)
{
    (void)unused;
    struct Invocation *call = gum_invocation_context_get_listener_invocation_data(context, sizeof(*call));
    if (!call->observed)
        return;
    struct Hook *hook = gum_invocation_context_get_listener_function_data(context);
    if (call->outer && hook->kind != FREE) {
        reporting = true;
        void *pointer = call->result_storage ? *call->result_storage : gum_invocation_context_get_return_value(context);
        if (pointer) {
            size_t bytes = allocation_size(pointer);
            if (!bytes)
                fail(hook->name, "PAS reported zero allocation size; allocator or ABI mismatch");
            reported_trace = &call->trace;
            if (hook->kind == REALLOC) {
                report_realloc(call->old_pointer, bytes, pointer);
                if (call->old_pointer)
                    atomic_fetch_add(&frees, 1);
            } else
                report_alloc(pointer, bytes);
            atomic_fetch_add(&allocations, 1);
            reported_trace = NULL;
        } else
            atomic_fetch_add(&failures, 1);
        reporting = false;
    }
    --depth;
}

void smabar_pas_hooks_start(const char *output, const char *heaptrack_library)
{
    if (started)
        fail("start", "already started");
    if (!output || !heaptrack_library)
        fail("start", "output and collector library are required");
    if (output[0] != '/' || strstr(output, "$$"))
        fail("start", "use an absolute output path without Heaptrack's $$ substitution");
    if (getenv("Malloc"))
        fail("start", "Malloc must be unset to preserve PAS");
    void *jsc = dlopen("libjavascriptcoregtk-4.1.so.0", RTLD_NOW | RTLD_LOCAL);
    if (!jsc)
        fail("load JSC", dlerror());
    unsigned (*major)(void) = required_symbol(jsc, "jsc_get_major_version");
    unsigned (*minor)(void) = required_symbol(jsc, "jsc_get_minor_version");
    unsigned (*micro)(void) = required_symbol(jsc, "jsc_get_micro_version");
    if (major() != 2 || minor() != 52 || micro() != 6)
        fail("start", "this diagnostic ABI requires WebKitGTK 2.52.6; revalidate before updating");
    bool (*pas_enabled)(void) = required_symbol(jsc, "_ZN3WTF19isFastMallocEnabledEv");
    if (!pas_enabled())
        fail("start", "PAS is configured to use the system allocator");
    allocation_size = required_symbol(jsc, "bmalloc_get_allocation_size");
    void *collector = dlopen(heaptrack_library, RTLD_NOW | RTLD_LOCAL);
    if (!collector)
        fail("load heaptrack", dlerror());
    void (*initialize)(const char *, void (*)(void), void (*)(void *), void (*)(void)) =
        required_symbol(collector, "heaptrack_init");
    report_alloc = required_symbol(collector, "heaptrack_malloc");
    report_free = required_symbol(collector, "heaptrack_free");
    report_realloc = required_symbol(collector, "heaptrack_realloc");
    stop_heaptrack = required_symbol(collector, "heaptrack_stop");
    unwind_address = required_symbol(collector, "unw_backtrace");
    int length = snprintf(stats_path, sizeof(stats_path), "%s.stats.json", output);
    if (length < 0 || (size_t)length >= sizeof(stats_path))
        fail("start", "output path too long");
    int trace_fd = reserve_output(output);
    stats_fd = reserve_output(stats_path);
    if (close(trace_fd))
        fail("close reserved trace", strerror(errno));
    initialize(output, NULL, collector_ready, NULL); /* No malloc/GOT replacement. */
    if (!collector_initialized)
        fail("initialize collector", "Heaptrack did not confirm opening and locking the trace");

    gum_init_embedded();
    interceptor = gum_interceptor_obtain();
    backtracer = gum_backtracer_make_accurate();
    if (!backtracer)
        fail("start", "accurate Gum backtracer unavailable");
    listener = gum_make_call_listener(on_enter, on_leave, NULL, NULL);
    gum_interceptor_begin_transaction(interceptor);
    GumReplaceReturn replace = gum_interceptor_replace_fast(interceptor, unwind_address,
        annotation_unwind, (void **)&original_unwind);
    if (replace != GUM_REPLACE_OK)
        fail("collector unwind bridge", "Gum rejected function replacement");
    for (size_t i = 0; i < sizeof(hooks) / sizeof(hooks[0]); ++i) {
        void *address = required_symbol(jsc, hooks[i].symbol);
        GumAttachReturn result = gum_interceptor_attach(interceptor, address, listener, &hooks[i], GUM_ATTACH_FLAGS_NONE);
        if (result != GUM_ATTACH_OK)
            fail(hooks[i].name, "Gum rejected allocator hook");
    }
    started = true;
    gum_interceptor_end_transaction(interceptor);
    fprintf(stderr, "PAS diagnostic ready pid=%d hooks=%zu output=%s\n", getpid(), sizeof(hooks) / sizeof(hooks[0]), output);
}

void smabar_pas_hooks_stop(void)
{
    if (!started)
        return;
    gum_interceptor_detach(interceptor, listener);
    /* Detached listeners can still have callbacks on other threads. Keep the
     * trace bridge and collector alive until Gum confirms their completion. */
    while (!gum_interceptor_flush(interceptor))
        g_usleep(1000);
    gum_interceptor_revert(interceptor, unwind_address);
    stop_heaptrack();
    FILE *file = fdopen(stats_fd, "w");
    if (!file)
        fail("write stats", strerror(errno));
    fprintf(file, "{\"allocations\":%" PRIu64 ",\"frees\":%" PRIu64 ",\"failures\":%" PRIu64 ",\"nestedIgnored\":%" PRIu64 ",\"hooks\":{",
        atomic_load(&allocations), atomic_load(&frees), atomic_load(&failures), atomic_load(&nested));
    for (size_t i = 0; i < sizeof(hooks) / sizeof(hooks[0]); ++i)
        fprintf(file, "%s\"%s\":%" PRIu64, i ? "," : "", hooks[i].name, atomic_load(&hooks[i].calls));
    if (fputs("}}\n", file) == EOF || fclose(file))
        fail("write stats", "output error");
    started = false;
}

/* Explicit activation only; the launcher passes both paths to child processes. */
__attribute__((constructor)) static void start_webprocess(void)
{
    const char *directory = getenv("SMABAR_PAS_TRACE_DIR");
    if (!directory || !*directory)
        return;
    char executable[PATH_MAX];
    ssize_t length = readlink("/proc/self/exe", executable, sizeof(executable) - 1);
    if (length < 0)
        fail("identify process", strerror(errno));
    executable[length] = '\0';
    const char *name = strrchr(executable, '/');
    if (!name)
        return;
    bool own_test = !strcmp(name + 1, "pas-hook-selftest") && getenv("SMABAR_PAS_TEST_CONSTRUCTOR");
    if (strcmp(name + 1, "WebKitWebProcess") && !own_test)
        return;
    const char *collector = getenv("SMABAR_PAS_HEAPTRACK");
    if (!collector || !*collector)
        fail("start WebProcess", "set SMABAR_PAS_HEAPTRACK to the checked Heaptrack 1.5.0 collector");
    char output[PATH_MAX];
    int size = snprintf(output, sizeof(output), "%s/pas-webkit-%d.raw", directory, getpid());
    if (size < 0 || (size_t)size >= sizeof(output))
        fail("start WebProcess", "trace directory too long");
    smabar_pas_hooks_start(output, collector); /* Before main: include page/cache construction. */
    if (atexit(smabar_pas_hooks_stop))
        fail("start WebProcess", "atexit registration failed");
}
