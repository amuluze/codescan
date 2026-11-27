import requests

# ruleid: py/requests-verify-false
requests.get("http://localhost", verify=False)
requests.get("http://localhost", verify=True)
