/* Test whether the host loader can use a shared library with the current search path. */
#include <dlfcn.h>

int main(int argc, char **argv) {
    if (argc != 2) {
        return 2;
    }
    void *handle = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (handle == 0) {
        return 1;
    }
    dlclose(handle);
    return 0;
}
