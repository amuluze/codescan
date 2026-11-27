from flask import request as req
import os


def unrelated_alias(config):
    # ruleid: py/os-system
    os.system(config.args)
