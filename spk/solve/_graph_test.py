# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

from .. import api, io
from . import graph, Solution


def test_resolve_build_same_result() -> None:

    # building a package and resolving an binary build
    # should both result in the same final state... this
    # ensures that builds are not attempted when one already exists

    base = graph.State.default()

    spec = api.Spec.from_dict({"pkg": "test/1.0.0"})
    build_spec = spec.clone()
    build_spec.update_for_build(api.OptionMap(), [])

    resolve = graph.ResolvePackage(build_spec, build_spec)
    build = graph.BuildPackage(spec, spec, Solution())

    with_binary = resolve.apply(base)
    with_build = build.apply(base)

    print("resolve")
    for change in resolve.iter_changes():
        print(io.format_change(change, 100))
    print("build")
    for change in build.iter_changes():
        print(io.format_change(change, 100))

    assert (
        with_binary.id == with_build.id
    ), "Build and resolve package should create the same final state"


def test_empty_options_do_not_unset() -> None:

    state = graph.State(
        pkg_requests=tuple(),
        var_requests=tuple(),
        packages=tuple(),
        options=tuple(),
    )

    assign_empty = graph.SetOptions(api.OptionMap({"something": ""}))
    assign_value = graph.SetOptions(api.OptionMap({"something": "value"}))

    new_state = assign_empty.apply(state)
    opts = new_state.get_option_map()
    assert opts["something"] == "", "should assign empty option of no current value"

    new_state = assign_value.apply(new_state)
    new_state = assign_empty.apply(new_state)
    opts = new_state.get_option_map()
    assert opts["something"] == "value", "should not unset value when one exists"
