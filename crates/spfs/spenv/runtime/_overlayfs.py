import os
import stat


def is_removed_entry(stat_result: os.stat_result) -> bool:

    # overlayfs uses character device files to denote
    # a file that was removed, using this special file
    # as a whiteout file of the same name.
    # - the device is always 0/0
    if not stat.S_ISCHR(stat_result.st_mode):
        return False
    device = stat_result.st_rdev
    major = os.major(device)
    minor = os.minor(device)
    return major == 0 and minor == 0
