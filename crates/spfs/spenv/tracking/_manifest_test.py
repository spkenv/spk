import os

from ._manifest import compute_tree, compute_manifest


def test_compute_tree_determinism():

    first = compute_tree("./spenv")
    second = compute_tree("./spenv")
    assert first == second


def test_compute_manifest():

    manifest = compute_manifest(os.path.abspath("./spenv"))
    assert manifest.get_path(__file__)
