from typing import Dict, List, DefaultDict
import shlex
import io
import subprocess

import pytest
from pytest_bdd import given, when, then, scenarios
from pytest_bdd.parsers import cfparse
import py.path

import spenv
import spenv.cli

# scenarios("../features")


# @pytest.yield_fixture
# def cmd_history() -> List:
#     return []


# def _parse_data_table(datatable_source: str) -> Dict[str, str]:

#     rows = datatable_source.strip().split("\n")
#     if not rows:
#         return {}
#     data: Dict[str, List[str]] = {}
#     header = rows.pop(0)
#     headers = _parse_data_table_row(header)
#     for header in headers:
#         data[header] = []

#     for row in range(len(rows)):
#         cols = _parse_data_table_row(rows[row])
#         for col in range(len(cols)):
#             data[headers[col]].append(cols[col])

#     return data


# def _parse_data_table_row(row_str: str) -> List[str]:

#     parts = row_str.strip().strip("|").split("|")
#     return [part.strip() for part in parts]


# @given(cfparse('that I have a layer called "{ref}"'))
# def have_a_layer_called(ref):

#     raise NotImplementedError()


# @given(cfparse('that there is an environment named "{ref}"'))
# def there_is_an_environment_called(ref: str, tmprepo: spenv.FileStore) -> None:

#     tmprepo.create_empty_layer(ref)


# @given(cfparse('that the repository has a layer called "{ref}"'))
# def the_repo_has_a_layer_called(
#     ref: str, datatable: str, tmprepo: spenv.FileStore
# ) -> None:

#     layer = tmprepo.create_empty_layer(ref)
#     files = _parse_data_table(datatable)
#     for filename in files["name"]:
#         py.path.local(layer.upperdir).join(filename).ensure()


# @given(cfparse('that I have a directory called "{dirname}"'))
# def have_a_directory_called(dirname: str, tmpdir: py.path.local) -> None:

#     tmpdir.join(dirname).ensure(dir=True)


# @when(cfparse('I run "{command}"'))
# def i_run_command(command, capsys, cmd_history):

#     code = _run_cmd_str(command)
#     output = capsys.readouterr()
#     assert code == 0, "command failed: " + output.err
#     cmd_history.append(
#         {"command": command, "code": code, "out": output.out, "err": output.err}
#     )


# @then(cfparse("I should be in a subshell"))
# def should_be_in_subshell(cmd_history):

#     assert cmd_history, "must have previous command to check"
#     assert cmd_history[-1]["code"] == 0, "command failed: " + cmd_history[-1]["command"]
#     # TODO: this is not actaully validating anything


# @then(cfparse('I should see "{expected}" in the output'))
# def see_in_the_output(cmd_history: List, expected: str) -> None:

#     last_cmd = cmd_history[-1]
#     assert expected in last_cmd["out"]


# def _run_cmd_str(command: str) -> int:

#     cmd = shlex.split(command)
#     assert cmd, "command cannot be empty"
#     assert cmd[0] == "spenv", "must be a spenv command"
#     return spenv.cli.main(cmd[1:])


# @then(cfparse('the directory "{dirname}" should not be empty'))
# def dir_should_not_be_empty(dirname: str) -> None:

#     assert py.path.local(dirname).exists()
#     assert py.path.local(dirname).listdir()


# @then(cfparse('"{ref}" should be in the repository'))
# def should_be_in_repository(ref):

#     raise NotImplementedError()
