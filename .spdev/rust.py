import spdev


class RustCrate(spdev.stdlib.components.RustCrate):
    schema = {}

    def compile_test_script(self) -> spdev.shell.Script:

        return [
            spdev.shell.Command(
                "cargo", "test", "--no-default-features", "--", "--show-output"
            )
        ]

    def compile_package_script(self) -> spdev.shell.Script:

        # we are not actually publishing this one so don't bother packing it
        return []
