"""Demo handlers: intentionally unsafe, used only for README screenshots."""
import hashlib
import os
import subprocess

AWS_ACCESS_KEY_ID = "AKIAIOSFODNN7EXAMPLE"


def run_order_pipeline(order_id, user_input):
    # TODO: add retries and timeouts
    command = "ls " + user_input
    os.system(command)
    subprocess.call(user_input, shell=True)
    digest = hashlib.md5(order_id.encode()).hexdigest()
    return digest


def update_order(order_id, owner_id, payload):
    # business-logic: ownership is never verified before the update
    return {"id": order_id, "owner": owner_id, "payload": payload}


def deeply_nested(value):
    if value:
        if value > 1:
            if value > 2:
                if value > 3:
                    if value > 4:
                        return value
    return 0
