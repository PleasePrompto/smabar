#define _GNU_SOURCE
#include <assert.h>
#include <dlfcn.h>
#include <stdbool.h>
#include <stdint.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum Kind { ALLOC, CALLOC, ALIGNED, REALLOC, STRDUP, MEMDUP, ISO, FREE };
struct Hook { const char *symbol; const char *name; enum Kind kind; bool sret; };
#define HOOK(symbol, name, kind, sret) {symbol, name, kind, sret},
static const struct Hook hooks[] = {
#include "pas-hooks.def"
};
#undef HOOK

/* Exact 2.52.6 layouts from bmalloc_type.h / pas_heap_ref_prefix.h. */
struct Type { unsigned size; unsigned alignment; const char *name; };
struct HeapRef { const struct Type *type; void *heap; unsigned allocator_index; bool non_compact; };
static const struct Type type = {96, 16, "PAS hook paired selftest"};
static struct HeapRef iso_heap = {&type, NULL, 0, true};
static struct HeapRef compact_heap = {&type, NULL, 0, false};
static void *jsc;
static void (*fast_free)(void *);
static void (*aligned_free)(void *);
static void (*iso_free)(void *);
static void *(*fast_alloc)(size_t);
static size_t (*pas_size)(const void *);

static void *symbol(const char *name)
{
    void *result = dlsym(jsc, name);
    if (!result) {
        fprintf(stderr, "selftest missing symbol %s: %s\n", name, dlerror());
        exit(2);
    }
    return result;
}

__attribute__((noinline)) static void allocation_marker_leaf(const struct Hook *hook)
{
    if (hook->kind == FREE)
        return;
    void *function = symbol(hook->symbol);
    void *pointer = NULL;
    void *old = NULL;
    switch (hook->kind) {
    case ALLOC:
        if (hook->sret)
            ((void (*)(void **, size_t))function)(&pointer, 73);
        else
            pointer = ((void *(*)(size_t))function)(73);
        break;
    case CALLOC:
        if (hook->sret)
            ((void (*)(void **, size_t, size_t))function)(&pointer, 3, 31);
        else
            pointer = ((void *(*)(size_t, size_t))function)(3, 31);
        break;
    case ALIGNED:
        pointer = ((void *(*)(size_t, size_t))function)(64, 193);
        assert((uintptr_t)pointer % 64 == 0);
        break;
    case REALLOC:
        old = fast_alloc(71);
        assert(old);
        memset(old, 0x5a, 71);
        if (hook->sret)
            ((void (*)(void **, void *, size_t))function)(&pointer, old, 291);
        else
            pointer = ((void *(*)(void *, size_t))function)(old, 291);
        assert(pointer && ((unsigned char *)pointer)[70] == 0x5a);
        break;
    case STRDUP:
        pointer = ((char *(*)(const char *))function)("controlled allocation");
        assert(pointer && !strcmp(pointer, "controlled allocation"));
        break;
    case MEMDUP:
        pointer = ((void *(*)(const void *, size_t))function)("controlled allocation", 22);
        assert(pointer && !memcmp(pointer, "controlled allocation", 22));
        break;
    case ISO:
        pointer = ((void *(*)(struct HeapRef *))function)(strstr(hook->name, "Compact") ? &compact_heap : &iso_heap);
        break;
    case FREE:
        abort();
    }
    assert(pointer && pas_size(pointer) > 0);
    if (hook->kind == CALLOC || strstr(hook->name, "Zeroed"))
        assert(((unsigned char *)pointer)[0] == 0);
    if (hook->kind == ALIGNED)
        aligned_free(pointer);
    else if (hook->kind == ISO)
        iso_free(pointer);
    else
        fast_free(pointer);
}

__attribute__((noinline)) static void allocation_marker_parent(void)
{
    for (size_t i = 0; i < sizeof(hooks) / sizeof(hooks[0]); ++i)
        allocation_marker_leaf(&hooks[i]);
}

static void *allocation_marker_worker(void *unused)
{
    (void)unused;
    for (unsigned i = 0; i < 4; ++i)
        allocation_marker_parent();
    return NULL;
}

__attribute__((noinline)) static void allocation_marker_zero(void)
{
    for (size_t i = 0; i < sizeof(hooks) / sizeof(hooks[0]); ++i) {
        const struct Hook *hook = &hooks[i];
        if (hook->kind != ALLOC && hook->kind != CALLOC && hook->kind != ALIGNED && hook->kind != REALLOC)
            continue;
        void *function = symbol(hook->symbol);
        void *pointer = NULL;
        if (hook->kind == ALLOC) {
            if (hook->sret)
                ((void (*)(void **, size_t))function)(&pointer, 0);
            else
                pointer = ((void *(*)(size_t))function)(0);
        } else if (hook->kind == REALLOC) {
            void *old = fast_alloc(71);
            if (hook->sret)
                ((void (*)(void **, void *, size_t))function)(&pointer, old, 0);
            else
                pointer = ((void *(*)(void *, size_t))function)(old, 0);
        } else {
            size_t first = hook->kind == ALIGNED ? 64 : 0;
            if (hook->sret)
                ((void (*)(void **, size_t, size_t))function)(&pointer, first, 0);
            else
                pointer = ((void *(*)(size_t, size_t))function)(first, 0);
        }
        assert(pointer && pas_size(pointer) > 0);
        if (hook->kind == ALIGNED)
            aligned_free(pointer);
        else
            fast_free(pointer);
    }
}

int main(int argc, char **argv)
{
    if (argc != 3) {
        fprintf(stderr, "usage: pas-hook-selftest OUTPUT COLLECTOR\n");
        return 2;
    }
    jsc = dlopen("libjavascriptcoregtk-4.1.so.0", RTLD_NOW | RTLD_LOCAL);
    assert(jsc);
    fast_free = symbol("_ZN3WTF8fastFreeEPv");
    aligned_free = symbol("_ZN3WTF15fastAlignedFreeEPv");
    iso_free = symbol("_ZN7bmalloc3api13isoDeallocateEPv");
    fast_alloc = symbol("_ZN3WTF10fastMallocEm");
    pas_size = symbol("bmalloc_get_allocation_size");
    bool (*pas_enabled)(void) = symbol("_ZN3WTF19isFastMallocEnabledEv");
    assert(pas_enabled());
    void (*start)(const char *, const char *) = dlsym(RTLD_DEFAULT, "smabar_pas_hooks_start");
    void (*stop)(void) = dlsym(RTLD_DEFAULT, "smabar_pas_hooks_stop");
    assert(start && stop);
    if (!getenv("SMABAR_PAS_TEST_CONSTRUCTOR"))
        start(argv[1], argv[2]);
    for (unsigned i = 0; i < 16; ++i)
        allocation_marker_parent();
    pthread_t workers[2];
    for (unsigned i = 0; i < 2; ++i)
        assert(!pthread_create(&workers[i], NULL, allocation_marker_worker, NULL));
    for (unsigned i = 0; i < 2; ++i)
        assert(!pthread_join(workers[i], NULL));
    allocation_marker_zero();

    /* Explicit sret failure and failed-realloc ownership checks. */
    void *result = (void *)(uintptr_t)1;
    void (*try_calloc)(void **, size_t, size_t) = symbol("_ZN3WTF13tryFastCallocEmm");
    try_calloc(&result, SIZE_MAX, 2);
    assert(!result);
    void *old = fast_alloc(71);
    memset(old, 0x63, 71);
    void (*try_realloc)(void **, void *, size_t) = symbol("_ZN3WTF14tryFastReallocEPvm");
    try_realloc(&result, old, SIZE_MAX / 2);
    assert(!result && ((unsigned char *)old)[70] == 0x63);
    fast_free(old);
    fast_free(NULL);
    assert(pas_enabled());
    stop();
    puts("PAS unchanged; 31 hooks exercised; paired and failed-realloc checks passed.");
    return 0;
}
