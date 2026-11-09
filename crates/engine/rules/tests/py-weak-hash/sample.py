import hashlib


def legacy_digest(data):
    # ruleid: py/weak-hash
    return hashlib.md5(data).hexdigest()


def weak_digest(data):
    # ruleid: py/weak-hash
    return hashlib.sha1(data).hexdigest()


def strong_digest(data):
    return hashlib.sha256(data).hexdigest()
