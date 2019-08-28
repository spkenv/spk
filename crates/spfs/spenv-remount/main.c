#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libgen.h>
#include <sys/mount.h>
#include <sys/stat.h>

#define MOUNT_TARGET "/env"

int is_mounted(const char *target)
{

    char *parent = malloc(strlen(target) + 1);
    strcpy(parent, target);
    parent = dirname(parent);

    struct stat st_parent;
    if (stat(parent, &st_parent) == -1)
    {
        return -1;
    }
    free(parent);

    struct stat st_target;
    if (stat(target, &st_target) == -1)
    {
        return -1;
    }

    return (st_target.st_dev != st_parent.st_dev);
}

int main(int argc, char *argv[])
{
    if (argc != 2)
    {
        printf("usage: spenv-remount OVERLAY_OPTIONS\n");
        return 1;
    }
    int result;

    result = is_mounted(MOUNT_TARGET);
    if (result == -1)
    {
        perror("Failed to check mount status");
        return 1;
    }
    if (!result)
    {
        printf(MOUNT_TARGET " is not mounted, cannot remount");
        return 1;
    }

    result = mount("overlay", MOUNT_TARGET, "overlay", 0, argv[1]);
    if (result != 0)
    {
        perror("Remount failed");
        return 1;
    }

    return result;
}
