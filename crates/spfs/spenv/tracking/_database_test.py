import os

from ._database import compute_tree, compute_db


def test_compute_tree_determinism():

    first = compute_tree("./spenv")
    second = compute_tree("./spenv")
    assert first == second


def test_compute_db():

    db = compute_db(os.path.abspath("./spenv"))
    assert db.get_path(__file__)
