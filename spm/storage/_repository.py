import abc


from .. import api

class UnknownPackageError(ValueError):
    pass

class Repository(metaclass=abc.ABCMeta):
    def publish_package_version(self, spec: api.Spec) -> None:

        pass
