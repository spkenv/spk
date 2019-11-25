#include "steps.c"

void print_usage()
{
    printf("run a command in a configured spenv namespace\n");
    printf("usage: spenv-enter LOWERDIR[:LOWERDIR...] COMMAND [ARGS...]\n");
}

step_t parse_args(int argc, char *argv[])
{
    if (argc < 3)
    {
        print_usage();
        return step_fail;
    }
    SPENV_LOWERDIRS = argv[1];
    SPENV_COMMAND = argv + 2;
    char *value = getenv("SPENV_DEBUG");
    SPENV_DEBUG = (value != NULL);
    return step_pass;
}

#define STEP_COUNT 11
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
