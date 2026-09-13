#define NATIVE_OBSERVER_SELFTEST
#include "webkit-native-observer.c"
#include <assert.h>
static void put_pointer(void *base, ptrdiff_t offset, uintptr_t value) { memcpy((char *)base + offset, &value, 8); }
static void put_word(void *base, ptrdiff_t offset, uint32_t value) { memcpy((char *)base + offset, &value, 4); }
int main(int argc, char **argv)
{
    if (argc != 2) return 2;
    start_observer(argv[1]); /* Real ELF/build guard, 11 Gum hooks, system GLib timer. */
    unsigned char cache[88] = {0}, queue[24] = {0}, backing[16 + 8 * 24] = {0};
    uintptr_t table = (uintptr_t)(backing + 16);
    put_pointer(cache, 0x10, table); put_word((void *)table, -12, 2); put_word((void *)table, -4, 8);
    put_word((void *)table, 0, 17); put_pointer((void *)table, 8, 0x1000); put_word((void *)table, 16, 4); put_word((void *)table, 20, 3);
    put_word((void *)table, 24, UINT32_MAX); /* Deleted bucket must be skipped. */
    put_word((void *)table, 48, 29); put_pointer((void *)table, 56, 0x2000); put_word((void *)table, 64, 4); put_word((void *)table, 68, 1);
    put_pointer(queue, 0, 51); put_pointer(queue, 8, 0x3000); put_word(queue, 20, 2);
    register_object((uintptr_t)cache, 0x1110, true, false);
    register_object((uintptr_t)queue, 0x2220, false, false);
    snapshot("synthetic-counts");
    assert(!layout_errors && !lifecycle_errors);
    int fd = open(request_path, O_WRONLY | O_CREAT | O_EXCL, 0600);
    assert(fd >= 0 && write(fd, "selftest-ack\n", 13) == 13 && close(fd) == 0);
    void *glib = dlopen("libglib-2.0.so.0", RTLD_NOW | RTLD_LOCAL);
    int (*iterate)(void *, int) = symbol(glib, "g_main_context_iteration");
    while (snapshot_id < 2) iterate(NULL, 1);
    assert(access(request_path, F_OK) != 0 && errno == ENOENT);
    struct Object *value = lookup((uintptr_t)cache);
    uint64_t first_id = value->id;
    value->address = 0; ++destroyed[0];
    register_object((uintptr_t)cache, 0x3330, true, false);
    assert(lookup((uintptr_t)cache)->id > first_id);
    snapshot("address-reuse");
    put_word((void *)table, -12, 3); snapshot("intentional-layout-error");
    assert(layout_errors == 1);
    put_word((void *)table, -12, 2);
    lookup((uintptr_t)cache)->tid += 1;
    snapshot("foreign-owner-skipped");
    puts("native observer headless selftest passed");
    return 0;
}
