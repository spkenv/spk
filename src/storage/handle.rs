use super::Repository;

#[derive(Debug)]
pub enum RepositoryHandle {
    SPFS(super::SPFSRepository),
    Mem(super::MemRepository),
}

impl RepositoryHandle {
    pub fn to_repo(self) -> Box<dyn Repository> {
        match self {
            Self::SPFS(repo) => Box::new(repo),
            Self::Mem(repo) => Box::new(repo),
        }
    }
}

impl std::ops::Deref for RepositoryHandle {
    type Target = dyn Repository;

    fn deref(&self) -> &Self::Target {
        match self {
            RepositoryHandle::SPFS(repo) => repo,
            RepositoryHandle::Mem(repo) => repo,
        }
    }
}

impl std::ops::DerefMut for RepositoryHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            RepositoryHandle::SPFS(repo) => repo,
            RepositoryHandle::Mem(repo) => repo,
        }
    }
}

impl From<super::SPFSRepository> for RepositoryHandle {
    fn from(repo: super::SPFSRepository) -> Self {
        RepositoryHandle::SPFS(repo)
    }
}

impl From<super::MemRepository> for RepositoryHandle {
    fn from(repo: super::MemRepository) -> Self {
        RepositoryHandle::Mem(repo)
    }
}
