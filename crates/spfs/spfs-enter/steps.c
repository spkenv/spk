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

#define SPFS_DIR "/spfs"
#define SHOTS_DIR "/shots"
#define RUNTIME_DIR "/tmp/spfs-runtime"
#define RUNTIME_UPPER_DIR "/tmp/spfs-runtime/upper"
#define RUNTIME_LOWER_DIR "/tmp/spfs-runtime/lower"
#define RUNTIME_WORK_DIR "/tmp/spfs-runtime/work"

char *SPFS_LOWERDIRS = NULL;
char **SPFS_COMMAND = NULL;
int SPFS_DEBUG = 0;
int SPFS_VIRTUALIZE_SHOTS = 0;
uid_t original_euid = -1;
uid_t original_uid = -1;

typedef int (*step_t)();

int step_pass() { return 0; }
int step_fail() { return 1; }

int enter_mount_namespace()
{
    if (unshare(CLONE_NEWNS) != 0)
    {
        perror("Failed to enter mount namespace");
        return 1;
    }
    return 0;
}

int privatize_existing_mounts()
{
    int result = mount("none", "/", NULL, MS_PRIVATE, NULL);
    if (result != 0)
    {
        perror("Failed to privatize existing mounts: /");
        return 1;
    }

    if (is_mounted("tmp")) {
        result = mount("none", "/tmp", NULL, MS_PRIVATE, NULL);
        if (result != 0)
        {
            perror("Failed to privatize existing mount: /tmp");
            return 1;
        }
    }

    if (!SPFS_VIRTUALIZE_SHOTS) {
        return 0;
    }
    if (is_mounted(SHOTS_DIR)) {
        result = mount("none", SHOTS_DIR, NULL, MS_PRIVATE, NULL);
        if (result != 0)
        {
            perror("Failed to privatize existing mount: "SHOTS_DIR);
            return 1;
        }
    }
    return 0;

}

int ensure_mount_targets_exist()
{
    int result;
    result = mkdir_permissive(SPFS_DIR);
    if (result != 0) {
        perror("Failed to create "SPFS_DIR);
        return 1;
    }
    result = mkdir_permissive(RUNTIME_DIR);
    if (result != 0) {
        perror("Failed to create "RUNTIME_DIR);
        return 1;
    }

}

int ensure_mounts_do_not_exist()
{
    int result = is_mounted(SPFS_DIR);
    if (result == -1)
    {
        perror("Failed to check for existing mount");
        return 1;
    }
    if (result)
    {
        printf("'%s' is already mounted, will not remount\n", SPFS_DIR);
        return 1;
    }
}

int become_root()
{

    original_euid = geteuid();
    int result = seteuid(0);
    if (result == -1) {
        perror("Failed to become root user (effective)");
        return 1;
    }
    original_uid = getuid();
    result = setuid(0);
    if (result == -1) {
        perror("Failed to become root user (actual)");
        return 1;
    }
}

int setup_runtime()
{
    int result;
    result = mount("none", RUNTIME_DIR, "tmpfs", MS_NOEXEC, 0);
    if (result != 0) {
        perror("Failed to mount "RUNTIME_DIR);
        return 1;
    }
    result = mkdir_permissive(RUNTIME_UPPER_DIR);
    if (result != 0) {
        perror("Failed to create "RUNTIME_UPPER_DIR);
        return 1;
    }
    result = mkdir_permissive(RUNTIME_LOWER_DIR);
    if (result != 0) {
        perror("Failed to create "RUNTIME_LOWER_DIR);
        return 1;
    }
    result = mkdir_permissive(RUNTIME_WORK_DIR);
    if (result != 0) {
        perror("Failed to create "RUNTIME_WORK_DIR);
        return 1;
    }
}

char *get_overlay_args()
{
    char *overlay_args = NULL;
    char *format_str =
        "lowerdir="RUNTIME_LOWER_DIR"%s%s"
        ",upperdir="RUNTIME_UPPER_DIR
        ",workdir="RUNTIME_WORK_DIR;
    char *separator = ":";
    size_t required_size = strlen(SPFS_LOWERDIRS);
    if (required_size == 0) separator = "";
    required_size += strlen(format_str);
    overlay_args = malloc(required_size + 1);
    sprintf(overlay_args, format_str, separator, SPFS_LOWERDIRS);
    return overlay_args;

}

int mount_env()
{

    char * overlay_args = get_overlay_args();
    if (SPFS_DEBUG) {
        fprintf(stderr, "/usr/bin/mount -t overlay -o %s none " SPFS_DIR, overlay_args);
    }
    int child_pid = fork();
    if (child_pid == 0) {
        execl("/usr/bin/mount", "/usr/bin/mount", "-t", "overlay", "-o", overlay_args, "none", SPFS_DIR, NULL);
    }
    if (child_pid < 0) {
        perror("Could not execute mount command");
        return 1;
    }
    int result;
    waitpid(child_pid, &result, 0);
    return result;

}

int mount_shots_if_necessary()
{

    if (!SPFS_VIRTUALIZE_SHOTS) {
        return 0;
    }

    int result;
    result = mount("none", SHOTS_DIR, "tmpfs", 0, 0);
    if (result != 0) {
        perror("Failed to mount "RUNTIME_DIR);
    }
    return result;

}

int become_original_user()
{
    int result = setuid(original_uid);
    if (result == -1) {
        perror("Failed to become regular user (actual)");
        return 1;
    }
    result = seteuid(original_euid);
    if (result == -1) {
        perror("Failed to become regular user (effective)");
        return 1;
    }
    return 0;

}

int drop_all_capabilities()
{
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

int run_command()
{
    return execv(SPFS_COMMAND[0], SPFS_COMMAND);
}

int is_mounted(const char *target)
{

    char *t = malloc(strlen(target) * sizeof(char) + sizeof(char));
    strcpy(t, target);
    char *parent = dirname(t);

    struct stat st_parent;
    if (stat(parent, &st_parent) == -1)
    {
        return -1;
    }
    free(t);

    struct stat st_target;
    if (stat(target, &st_target) == -1)
    {
        return -1;
    }

    return (st_target.st_dev != st_parent.st_dev);
}

int mkdir_permissive(const char *path)
{

    int result;
    result = mkdir(path, S_IRWXU | S_IRWXG | S_IRWXO);
    if (result == -1)
    {
        if (errno == EEXIST)
        {
            return 0;
        }
        return -1;
    }

    // the above creation mode is affected by the current umask
    result = chmod(path, S_IRWXU | S_IRWXG | S_IRWXO);
    if (result == -1)
    {
        return -1;
    }

    return 0;
}
