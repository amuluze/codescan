import yaml


def read_config(text):
    # ruleid: py/yaml-load-without-loader
    return yaml.load(text)


def read_safe(text):
    data = yaml.load(text, Loader=yaml.SafeLoader)
    return data


def read_csafe(text):
    return yaml.load(text, Loader=yaml.CSafeLoader)


def read_safe_positional(text):
    return yaml.load(text, yaml.SafeLoader)


def read_csafe_positional(text):
    return yaml.load(text, yaml.CSafeLoader)


def read_none_loader(text):
    # ruleid: py/yaml-load-without-loader
    return yaml.load(text, Loader=None)


def read_unsafe_loader(text):
    # ruleid: py/yaml-load-without-loader
    return yaml.load(text, Loader=yaml.UnsafeLoader)


def read_typed(text):
    return yaml.safe_load(text)
