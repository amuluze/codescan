import yaml


def read_config(text):
    # ruleid: py/yaml-load-without-loader
    return yaml.load(text)


def read_safe(text):
    data = yaml.load(text, Loader=yaml.SafeLoader)
    return data


def read_typed(text):
    return yaml.safe_load(text)
