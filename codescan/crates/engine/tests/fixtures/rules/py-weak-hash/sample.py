import hashlib
import hashlib as h
from hashlib import md5
from hashlib import sha1 as weak_sha1


def legacy_digest(data):
    # ruleid: py/weak-hash
    return hashlib.md5(data).hexdigest()


def weak_digest(data):
    # ruleid: py/weak-hash
    return hashlib.sha1(data).hexdigest()


def aliased_digest(data):
    # ruleid: py/weak-hash
    return h.md5(data).hexdigest()


def imported_digest(data):
    # ruleid: py/weak-hash
    return md5(data).hexdigest()


def renamed_digest(data):
    # ruleid: py/weak-hash
    return weak_sha1(data).hexdigest()


def strong_digest(data):
    return hashlib.sha256(data).hexdigest()
