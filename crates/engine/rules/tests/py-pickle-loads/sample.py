import json
import pickle


def load_session(blob):
    # ruleid: py/pickle-loads
    return pickle.loads(blob)


def restore_stream(stream):
    # ruleid: py/pickle-loads
    return pickle.load(stream)


def parse_text(text):
    return json.loads(text)
