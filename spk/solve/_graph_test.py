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
