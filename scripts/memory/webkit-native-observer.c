#define _GNU_SOURCE
#include "frida-gum.h"
#include <dlfcn.h>
#include <elf.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <limits.h>
#include <link.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

/* Private ABI, guarded by the exact installed x86-64 WebKit build ID. */
enum Action { CACHE_CTOR, CACHE_DTOR, CACHE_CHANGE, QUEUE_CTOR, QUEUE_DTOR, QUEUE_CHANGE };
struct Hook { const char *name; uintptr_t offset; enum Action action; uint64_t calls; };
static struct Hook hooks[] = {
    {"cache_ctor", 0x3810660, CACHE_CTOR, 0},
    {"cache_dtor", 0x3810d20, CACHE_DTOR, 0},
    {"cache_add", 0x3811690, CACHE_CHANGE, 0},
    {"cache_sweep", 0x3810760, CACHE_CHANGE, 0},
    {"cache_invalidate", 0x3811c70, CACHE_CHANGE, 0},
    {"cache_remove", 0x3811c00, CACHE_CHANGE, 0},
    {"cache_viewport_clear", 0x3811d60, CACHE_CHANGE, 0},
    {"queue_context_ctor", 0x301c870, QUEUE_CTOR, 0},
    {"queue_dtor", 0x3021520, QUEUE_DTOR, 0},
    {"queue_append", 0x301ebb0, QUEUE_CHANGE, 0},
    {"queue_clear", 0x2866360, QUEUE_CHANGE, 0},
};
struct Object { uintptr_t address, owner; uint64_t id; long tid; bool cache, destroying; };
struct Call { uintptr_t object, owner; struct Hook *hook; bool observed; };
static struct Object objects[4096];
static size_t object_count;
static uint64_t next_id, snapshot_id, lifecycle_errors, thread_errors, layout_errors, unknown_objects;
static uint64_t created[2], destroyed[2];
static uintptr_t module_base, module_end;
static bool correct_build;
static FILE *output;
static char request_path[PATH_MAX];
static long main_tid;
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static _Thread_local unsigned hook_depth;
static GumInterceptor *interceptor;
static GumInvocationListener *listener;

static double now(void)
{
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts)) _exit(127);
    return ts.tv_sec + ts.tv_nsec / 1e9;
}
static long tid(void) { return syscall(SYS_gettid); }
static uintptr_t pointer_at(uintptr_t base, ptrdiff_t offset)
{
    uintptr_t result; memcpy(&result, (void *)(base + offset), sizeof(result)); return result;
}
static uint32_t word_at(uintptr_t base, ptrdiff_t offset)
{
    uint32_t result; memcpy(&result, (void *)(base + offset), sizeof(result)); return result;
}
static void fail(const char *message)
{
    fprintf(stderr, "Native observer failed: %s (%s)\n", message, strerror(errno));
    _exit(127);
}
static void *symbol(void *module, const char *name)
{
    dlerror(); void *value = dlsym(module, name);
    if (dlerror() || !value) fail(name);
    return value;
}
static struct Object *lookup(uintptr_t address)
{
    for (size_t i = 0; i < object_count; ++i)
        if (objects[i].address == address) return &objects[i];
    return NULL;
}
static struct Object *register_object(uintptr_t address, uintptr_t owner, bool cache, bool discovered)
{
    struct Object *value = lookup(address);
    if (value) { ++lifecycle_errors; return value; }
    size_t index = 0;
    while (index < object_count && objects[index].address) ++index;
    if (index == sizeof(objects) / sizeof(objects[0])) { ++lifecycle_errors; return NULL; }
    if (index == object_count) ++object_count;
    value = &objects[index];
    *value = (struct Object){address, owner, ++next_id, tid(), cache, false};
    ++created[cache ? 0 : 1];
    if (discovered) ++unknown_objects;
    return value;
}
static void on_enter(GumInvocationContext *context, gpointer unused)
{
    (void)unused;
    struct Call *call = gum_invocation_context_get_listener_invocation_data(context, sizeof(*call));
    memset(call, 0, sizeof(*call));
    call->hook = gum_invocation_context_get_listener_function_data(context);
    call->object = (uintptr_t)gum_invocation_context_get_nth_argument(context, 0);
    call->owner = (uintptr_t)gum_invocation_context_get_nth_argument(context, 1);
    call->observed = true;
    ++hook_depth;
    if (call->hook->action == QUEUE_CTOR) call->object += 0x118;
    pthread_mutex_lock(&lock);
    ++call->hook->calls;
    if (call->hook->action != CACHE_CTOR && call->hook->action != QUEUE_CTOR) {
        struct Object *value = lookup(call->object);
        if (!value && (call->hook->action == CACHE_CHANGE || call->hook->action == QUEUE_CHANGE))
            value = register_object(call->object, 0, call->hook->action == CACHE_CHANGE, true);
        if (value) {
            if (value->tid != tid()) ++thread_errors;
            if (call->hook->action == CACHE_DTOR || call->hook->action == QUEUE_DTOR) value->destroying = true;
        } else ++lifecycle_errors;
    }
    pthread_mutex_unlock(&lock);
}
static void on_leave(GumInvocationContext *context, gpointer unused)
{
    (void)unused;
    struct Call *call = gum_invocation_context_get_listener_invocation_data(context, sizeof(*call));
    if (!call->observed) return;
    pthread_mutex_lock(&lock);
    enum Action action = call->hook->action;
    if (action == CACHE_CTOR || action == QUEUE_CTOR)
        register_object(call->object, call->owner, action == CACHE_CTOR, false);
    if (action == CACHE_DTOR || action == QUEUE_DTOR) {
        struct Object *value = lookup(call->object);
        if (value) { ++destroyed[value->cache ? 0 : 1]; value->address = 0; }
        else ++lifecycle_errors;
    }
    pthread_mutex_unlock(&lock);
    --hook_depth;
}
struct Counts { uint64_t caches, queues, keys, entries, renderers, foreign, destroying; };
static void inspect_cache(struct Object *object, struct Counts *totals)
{
    uintptr_t table = pointer_at(object->address, 0x10);
    uint32_t keys = table ? word_at(table, -12) : 0, slots = table ? word_at(table, -4) : 0;
    uint64_t entries = 0, seen_keys = 0;
    if (keys > 16384 || slots > 65536 || (slots && (slots & (slots - 1)))) { ++layout_errors; return; }
    for (uint32_t i = 0; i < slots; ++i) {
        uintptr_t slot = table + i * 24;
        uint32_t key = word_at(slot, 0);
        if (!key || key == UINT32_MAX) continue;
        ++seen_keys;
        uintptr_t vector = pointer_at(slot, 8);
        uint32_t count = word_at(slot, 20), capacity = word_at(slot, 16);
        if (count > 4 || count > capacity || (count && !vector)) { ++layout_errors; continue; }
        entries += count;
    }
    if (seen_keys != keys) ++layout_errors;
    ++totals->caches; totals->keys += keys; totals->entries += entries;
    fprintf(output, "{\"event\":\"cache\",\"snapshot_id\":%" PRIu64 ",\"id\":%" PRIu64 ",\"address\":\"0x%" PRIxPTR "\",\"resolver\":\"0x%" PRIxPTR "\",\"owner_tid\":%ld,\"keys\":%u,\"slots\":%u,\"entries\":%" PRIu64 ",\"additions_since_sweep\":%u}\n", snapshot_id, object->id, object->address, object->owner, object->tid, keys, slots, entries, word_at(object->address, 0x50));
}
static void inspect_queue(struct Object *object, struct Counts *totals)
{
    uint64_t count = pointer_at(object->address, 0);
    uintptr_t segments = pointer_at(object->address, 8);
    uint32_t segment_count = word_at(object->address, 20);
    if (count > 5000 || segment_count > 100 || count > (uint64_t)segment_count * 50 || (count && !segments)) { ++layout_errors; return; }
    ++totals->queues; totals->renderers += count;
    fprintf(output, "{\"event\":\"queue\",\"snapshot_id\":%" PRIu64 ",\"id\":%" PRIu64 ",\"address\":\"0x%" PRIxPTR "\",\"frame_view\":\"0x%" PRIxPTR "\",\"owner_tid\":%ld,\"count\":%" PRIu64 ",\"segments\":%u}\n", snapshot_id, object->id, object->address, object->owner, object->tid, count, segment_count);
}
static void snapshot(const char *label)
{
    pthread_mutex_lock(&lock);
    if (hook_depth) { pthread_mutex_unlock(&lock); return; }
    ++snapshot_id;
    struct Counts totals = {0};
    double started = now();
    long reader_tid = tid();
    for (size_t i = 0; i < object_count; ++i) {
        struct Object *object = &objects[i];
        if (!object->address) continue;
        if (object->destroying) { ++totals.destroying; continue; }
        if (object->tid != reader_tid) { ++totals.foreign; continue; }
        if (object->cache) inspect_cache(object, &totals); else inspect_queue(object, &totals);
    }
    fprintf(output, "{\"event\":\"snapshot\",\"snapshot_id\":%" PRIu64 ",\"monotonic_seconds\":%.9f,\"label\":\"%s\",\"reader_tid\":%ld,\"main_tid\":%ld,\"caches\":%" PRIu64 ",\"queues\":%" PRIu64 ",\"cache_keys\":%" PRIu64 ",\"cache_entries\":%" PRIu64 ",\"queue_objects\":%" PRIu64 ",\"foreign_owner_objects\":%" PRIu64 ",\"destroying_objects\":%" PRIu64 ",\"lifecycle_errors\":%" PRIu64 ",\"thread_errors\":%" PRIu64 ",\"layout_errors\":%" PRIu64 ",\"discovered_without_ctor\":%" PRIu64 ",\"cache_created\":%" PRIu64 ",\"cache_destroyed\":%" PRIu64 ",\"queue_created\":%" PRIu64 ",\"queue_destroyed\":%" PRIu64 ",\"duration_ms\":%.3f,\"hooks\":{", snapshot_id, started, label, reader_tid, main_tid, totals.caches, totals.queues, totals.keys, totals.entries, totals.renderers, totals.foreign, totals.destroying, lifecycle_errors, thread_errors, layout_errors, unknown_objects, created[0], destroyed[0], created[1], destroyed[1], (now() - started) * 1000);
    for (size_t i = 0; i < sizeof(hooks) / sizeof(hooks[0]); ++i) fprintf(output, "%s\"%s\":%" PRIu64, i ? "," : "", hooks[i].name, hooks[i].calls);
    fputs("}}\n", output);
    if (fflush(output) || ferror(output)) fail("write snapshot");
    pthread_mutex_unlock(&lock);
}
static int timer_callback(void *unused)
{
    (void)unused;
    if (hook_depth) return 1;
    char label[65] = "periodic";
    int fd = open(request_path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW);
    if (fd >= 0) {
        struct stat info;
        if (fstat(fd, &info) || !S_ISREG(info.st_mode) || info.st_uid != getuid() || (info.st_mode & 0777) != 0600 || info.st_size < 1 || info.st_size > 64) fail("invalid snapshot request file");
        ssize_t length = read(fd, label, 64);
        if (length < 1) fail("read snapshot request");
        label[length] = 0;
        while (length > 0 && (label[length - 1] == '\n' || label[length - 1] == '\r')) label[--length] = 0;
        if (!length || strspn(label, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-") != (size_t)length) fail("invalid snapshot label");
        if (close(fd) || unlink(request_path)) fail("consume snapshot request");
    } else if (errno != ENOENT) fail("open snapshot request");
    snapshot(label);
    return 1;
}
static int locate_module(struct dl_phdr_info *info, size_t size, void *unused)
{
    (void)size; (void)unused;
    if (!strstr(info->dlpi_name, "libwebkit2gtk-4.1.so.0")) return 0;
    static const unsigned char expected[] = {0x6d,0x0d,0xb8,0x77,0xd1,0xbd,0xa6,0x55,0x39,0xbf,0x0c,0x84,0xfd,0x6f,0x72,0x77,0x3b,0x56,0xed,0x4b};
    module_base = info->dlpi_addr;
    for (unsigned i = 0; i < info->dlpi_phnum; ++i) {
        const ElfW(Phdr) *ph = &info->dlpi_phdr[i];
        if (ph->p_type == PT_LOAD && module_base + ph->p_vaddr + ph->p_memsz > module_end) module_end = module_base + ph->p_vaddr + ph->p_memsz;
        if (ph->p_type != PT_NOTE) continue;
        const unsigned char *cursor = (void *)(module_base + ph->p_vaddr), *end = cursor + ph->p_memsz;
        while ((size_t)(end - cursor) >= sizeof(ElfW(Nhdr))) {
            const ElfW(Nhdr) *note = (void *)cursor;
            size_t names = (note->n_namesz + 3U) & ~3U, desc = (note->n_descsz + 3U) & ~3U;
            if (sizeof(*note) + names + desc > (size_t)(end - cursor)) break;
            const unsigned char *name = cursor + sizeof(*note), *value = name + names;
            if (note->n_type == NT_GNU_BUILD_ID && note->n_namesz == 4 && !memcmp(name, "GNU", 4) && note->n_descsz == sizeof(expected)) correct_build = !memcmp(value, expected, sizeof(expected));
            cursor += sizeof(*note) + names + desc;
        }
    }
    return 1;
}
static void start_observer(const char *directory)
{
    if (!dlopen("libwebkit2gtk-4.1.so.0", RTLD_NOW | RTLD_LOCAL)) fail("load WebKit");
    dl_iterate_phdr(locate_module, NULL);
    if (!module_base || !correct_build) fail("unsupported WebKit build ID");
    void *jsc = dlopen("libjavascriptcoregtk-4.1.so.0", RTLD_NOW | RTLD_LOCAL);
    if (!jsc) fail("load JSC");
    bool (*pas_enabled)(void) = symbol(jsc, "_ZN3WTF19isFastMallocEnabledEv");
    if (getenv("Malloc") || !pas_enabled()) fail("normal PAS allocator required");
    char path[PATH_MAX];
    int a = snprintf(path, sizeof(path), "%s/native-observer-%d.jsonl", directory, getpid());
    int b = snprintf(request_path, sizeof(request_path), "%s/native-observer-%d.request", directory, getpid());
    if (a < 0 || b < 0 || a >= (int)sizeof(path) || b >= (int)sizeof(request_path)) fail("output path too long");
    int fd = open(path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC | O_NOFOLLOW, 0600);
    if (fd < 0 || !(output = fdopen(fd, "w"))) fail("reserve new output");
    main_tid = tid();
    gum_init_embedded(); interceptor = gum_interceptor_obtain();
    listener = gum_make_call_listener(on_enter, on_leave, NULL, NULL);
    gum_interceptor_begin_transaction(interceptor);
    for (size_t i = 0; i < sizeof(hooks) / sizeof(hooks[0]); ++i) {
        void *address = (void *)(module_base + hooks[i].offset);
        if (memcmp(address, "\xf3\x0f\x1e\xfa\x55", 5)) fail("hook prologue mismatch");
        if (gum_interceptor_attach(interceptor, address, listener, &hooks[i], GUM_ATTACH_FLAGS_NONE) != GUM_ATTACH_OK) fail(hooks[i].name);
    }
    gum_interceptor_end_transaction(interceptor);
    void *glib = dlopen("libglib-2.0.so.0", RTLD_NOW | RTLD_LOCAL);
    if (!glib) fail("load system GLib");
    unsigned (*timeout_add)(int, unsigned, int (*)(void *), void *, void (*)(void *)) = symbol(glib, "g_timeout_add_full");
    unsigned interval = 1000;
    const char *setting = getenv("SMABAR_NATIVE_OBSERVER_INTERVAL_MS");
    if (setting) {
        char *end; unsigned long value = strtoul(setting, &end, 10);
        if (!*setting || *end || value < 100 || value > 60000) fail("invalid snapshot interval");
        interval = (unsigned)value;
    }
    unsigned source = timeout_add(0, interval, timer_callback, NULL, NULL);
    if (!source) fail("register system GLib timer");
    fprintf(output, "{\"event\":\"ready\",\"pid\":%d,\"main_tid\":%ld,\"monotonic_seconds\":%.9f,\"build_id\":\"6d0db877d1bda65539bf0c84fd6f72773b56ed4b\",\"module_base\":\"0x%" PRIxPTR "\",\"hooks\":%zu,\"interval_ms\":%u,\"system_glib_source\":%u}\n", getpid(), main_tid, now(), module_base, sizeof(hooks) / sizeof(hooks[0]), interval, source);
    if (fflush(output)) fail("write ready");
    fprintf(stderr, "Native observer ready pid=%d hooks=%zu output=%s\n", getpid(), sizeof(hooks) / sizeof(hooks[0]), path);
}
#ifndef NATIVE_OBSERVER_SELFTEST
__attribute__((constructor)) static void constructor(void)
{
    const char *directory = getenv("SMABAR_MEMORY_TRACE_DIR");
    if (!directory) return;
    char executable[PATH_MAX];
    ssize_t length = readlink("/proc/self/exe", executable, sizeof(executable) - 1);
    if (length < 0) return;
    executable[length] = 0;
    const char *name = strrchr(executable, '/');
    if (name && !strcmp(name + 1, "WebKitWebProcess")) start_observer(directory);
}
#endif
