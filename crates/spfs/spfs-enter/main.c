#include "steps.c"

void print_usage()
{
    printf("run a command in a configured spfs namespace\n\n");
    printf("usage: spfs-enter LOWERDIR[:LOWERDIR...] COMMAND [ARGS...]\n\n");
    printf("Use the following environment variables for additional configuration:\n");
    printf("  SPFS_DEBUG: if set, print debugging output\n");
    printf("  SPFS_VIRTUALIZE_SHOTS: if set, mount a temporary file system over /shots\n");
    printf("                          (/shots must be a directory, not a symlink)\n");
}

step_t parse_args(int argc, char *argv[])
{
    if (argc < 3)
    {
        print_usage();
        return step_fail;
    }
    SPFS_LOWERDIRS = argv[1];
    SPFS_COMMAND = argv + 2;
    SPFS_DEBUG = (getenv("SPFS_DEBUG") != NULL);
    SPFS_VIRTUALIZE_SHOTS = (getenv("SPFS_VIRTUALIZE_SHOTS") != NULL);
    return step_pass;
}

#define STEP_COUNT 12
int main(int argc, char *argv[])
{
    step_t steps[STEP_COUNT] = {
        parse_args(argc, argv),
        enter_mount_namespace,
        privatize_existing_mounts,
        ensure_mount_targets_exist,
        ensure_mounts_do_not_exist,
        become_root,
        setup_runtime,
        mount_env,
        mount_shots_if_necessary,
        become_original_user,
        drop_all_capabilities,
        run_command,
    };
    int result, i;
    for (i = 0; i < STEP_COUNT; i++) {
        result = steps[i]();
        if (result != 0) {
            break;
        }
    }
    return result;

}
