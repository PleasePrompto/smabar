/* Headless selftest fault only: make the real collector fail its output lock. */
#include <errno.h>
#include <sys/file.h>

int flock(int file, int operation)
{
    (void)file;
    (void)operation;
    errno = EWOULDBLOCK;
    return -1;
}
