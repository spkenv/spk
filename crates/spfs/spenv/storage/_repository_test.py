import pytest
import git

from ._repository import Repository, RepositoryStorage, UnknownVersionError


def test_unknown_version_pattern():

    assert UnknownVersionError.pattern.match("fatal: invalid reference: 25\n")


def test_repository_add_worktree(tmpdir):

    repo_root = tmpdir.join("repo.git").ensure(dir=True)
    git_repo = git.Repo.init(repo_root.strpath, bare=True)
    git_repo.index.commit("Initial Commit")
    repo = Repository(repo_root.strpath)

    tmptree = tmpdir.join("worktree").ensure(dir=True)
    created = repo.add_worktree(tmptree.strpath, "HEAD")

    assert created.head.name == "HEAD"
    assert created.bare == False
    assert created.has_separate_working_tree()


def test_repository_add_worktree_noref(tmpdir):

    repos = RepositoryStorage(tmpdir.strpath)
    repo = repos.create_repository("repo")
    with pytest.raises(UnknownVersionError):
        repo.add_worktree(tmpdir.join("work").strpath, "HEAD")


def test_storage_read_repository_not_exist(tmpdir):

    repos = RepositoryStorage(tmpdir.strpath)

    with pytest.raises(ValueError):
        repos.read_repository("gitlab.spimageworks.com/spi/base")


def test_storage_read_repository_exists(tmpdir):

    repos = RepositoryStorage(tmpdir.strpath)
    repo_dir = tmpdir.join("gitlab.spimageworks.com/spi/base.git").ensure(dir=True)
    git.Repo.init(repo_dir, bare=True)
    repo = repos.read_repository("gitlab.spimageworks.com/spi/base")
    assert repo
