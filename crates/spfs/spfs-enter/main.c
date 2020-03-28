#include "steps.c"

#define OPT_FLAGS ":vserd:"
void print_usage()
{
    printf("run a command in a configured spfs runtime\n\n");
    printf("usage: spfs-enter -evsr [-d LOWERDIR ...] COMMAND [ARGS...]\n\n");
    printf("options:\n");
    printf("  -e: Make the mount editable with an in-memory upper and workdir\n");
    printf("  -v: Enable verbose output (can also be specified by setting the SPFS_DEBUG env var)\n");
    printf("  -s: Also virtualize the /shots directory by mounting a tempfs over it\n");
    printf("  -r: Remount the overlay filesystem, don't enter a new namepace\n");
    printf("  -d LOWERDIR: Include the given directory in the overlay mount\n");
    printf("     (can be specified more than once)\n\n");
    printf("Use the following environment variables for additional configuration:\n");
    printf("  SPFS_DEBUG: if set, print debugging output\n");
}

#define STEP_COUNT 12
int main(int argc, char *argv[])
{
    SPFS_DEBUG = (getenv("SPFS_DEBUG") != NULL);

    int opt;
    while ((opt = getopt(argc, argv, OPT_FLAGS)) != -1) {
        switch (opt) {
            case 'e':
                SPFS_EDITABLE = 1;
                break;
            case 'v':
                SPFS_DEBUG = 1;
                break;
            case 's':
                SPFS_VIRTUALIZE_SHOTS = 1;
                break;
            case 'r':
                SPFS_REMOUNT_ONLY = 1;
                break;
            case 'd':
                if(SPFS_LOWERDIRS == NULL) {
                    SPFS_LOWERDIRS = optarg;
                    break;
                }
                char *existing = SPFS_LOWERDIRS;
                size_t required_size = strlen(SPFS_LOWERDIRS) + strlen(optarg);
                SPFS_LOWERDIRS = malloc(required_size + 2);
                sprintf(SPFS_LOWERDIRS, "%s:%s", existing, optarg);
                break;
            case ':':
                printf("value required for option '%c'\n", optopt);
                print_usage();
                return 1;
            case '?':
                printf("unknown option: '%c'\n", optopt);
                print_usage();
                return 1;
            default:
                printf("unhandled option %s\n", optopt);
                return 1;
        }
    }

    if (optind >= argc && !SPFS_REMOUNT_ONLY) {
        print_usage();
        return 1;
    }
    else if (optind != argc && SPFS_REMOUNT_ONLY) {
        print_usage();
        return 1;
    }
    SPFS_COMMAND = argv + optind;

    step_t remount_steps[] = {
        become_root,
        ensure_mounts_already_exist,
        setup_runtime,
        mount_env,
        mount_shots_if_necessary,
        become_original_user,
        drop_all_capabilities,
        NULL,
    };
    step_t enter_steps[] = {
        become_root,
        enter_mount_namespace,
        privatize_existing_mounts,
        ensure_mount_targets_exist,
        setup_runtime,
        mount_env,
        mount_shots_if_necessary,
        become_original_user,
        drop_all_capabilities,
        run_command,
        NULL,
    };

    step_t *steps = NULL;
    if (SPFS_REMOUNT_ONLY) {
        steps = &remount_steps[0];
    }
    else
    {
        steps = &enter_steps[0];
    }

    int result, i;
    for (i = 0; steps[i] != NULL; i++) {
        result = steps[i]();
        if (result != 0) {
            break;
        }
    }
    return result;

}
