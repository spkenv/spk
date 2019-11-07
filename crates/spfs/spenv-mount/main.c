#define _GNU_SOURCE
#include <errno.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/prctl.h>
#include <sys/capability.h>
#include <sys/wait.h>
#include <unistd.h>
#include <string.h>
#include <libgen.h>

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

int create_mount_target()
{

    int result;
    result = mkdir(MOUNT_TARGET, S_IRWXU | S_IRWXG | S_IRWXO);
    if (result == -1)
    {
        if (errno == EEXIST)
        {
            return 0;
        }
        return -1;
    }

    // the above creation mode is affected by the current umask
    result = chmod(MOUNT_TARGET, S_IRWXU | S_IRWXG | S_IRWXO);
    if (result == -1)
    {
        return -1;
    }

    return 0;
}


int drop_all_caps() {

    cap_t capabilities = cap_get_proc();
    int result = cap_clear(capabilities);
    if (result != 0)
    {
        return -1;
    }
    result = cap_set_proc(capabilities);
    if (result != 0)
    {
        return -1;
    }
    result = cap_free(capabilities);
    if (result != 0)
    {
        return -1;
    }
    return 0;

}


int main(int argc, char *argv[])
{
    if (argc < 3)
    {
        printf("usage: spenv-mount OVERLAY_OPTIONS COMMAND\n");
        return 1;
    }
    int result;
    result = unshare(CLONE_NEWNS);
    if (result != 0)
    {
        perror("Failed to enter mount namespace");
        return 1;
    }

    result = mount("none", "/", NULL, MS_PRIVATE, NULL);
    if (result != 0)
    {
        perror("Failed to privatize existing mounts");
        return 1;
    }

    result = create_mount_target();
    if (result == -1)
    {
        perror("Failed to create " MOUNT_TARGET);
        return 1;
    }

    result = is_mounted(MOUNT_TARGET);
    if (result == -1)
    {
        perror("Failed to check for existing mount");
        return 1;
    }
    if (result)
    {
        printf("'%s' is already mounted, will not remount\n", MOUNT_TARGET);
        return 1;
    }

    uid_t original_uid = getuid();
    result = setuid(0);
    if (result == -1) {
        perror("Failed to become root user");
        return 1;
    }

    // TODO: investigate why the direct mount() call causes permission
    // issues, when use of the mount command line does not
    //result = mount("none", MOUNT_TARGET, "overlay", 0, argv[1]);
    int child_pid = fork();
    if (child_pid == 0) {
        execl("/usr/bin/mount", "/usr/bin/mount", "-t", "overlay", "-o", argv[1], "none", MOUNT_TARGET, NULL);
    }
    if (child_pid < 0) {
        perror("Could not execute mount command");
        return 1;
    }

    waitpid(child_pid, &result, 0);
    if (result != 0) {
        return result;
    }

    result = setuid(original_uid);
    if (result == -1) {
        perror("Failed to become regular user");
        return 1;
    }

    result = drop_all_caps();
    if (result -= 0) {
        perror("Failed to drop capabilities");
        return 1;
    }

    execv(argv[2], argv+2);

}
