import os
import pickle
import subprocess

import requests
import yaml


def evals(user_input):
    # ruleid: pmx-py-eval
    eval(user_input)
    # ok: pmx-py-eval
    eval("1 + 1")
    # ruleid: pmx-py-eval
    exec(user_input)


def commands(name):
    # ruleid: pmx-py-command-injection
    subprocess.run(f"git log {name}", shell=True)
    # ok: pmx-py-command-injection
    subprocess.run(["git", "log", name])
    # ok: pmx-py-command-injection
    subprocess.run("git status", shell=True)
    # ruleid: pmx-py-command-injection
    os.system("rm -rf " + name)
    # ok: pmx-py-command-injection
    os.system("ls")


def sql(cursor, user_id):
    # ruleid: pmx-py-sql-injection
    cursor.execute("SELECT * FROM users WHERE id = %s" % user_id)
    # ruleid: pmx-py-sql-injection
    cursor.execute(f"SELECT * FROM users WHERE id = {user_id}")
    # ruleid: pmx-py-sql-injection
    cursor.execute("DELETE FROM users WHERE id = {}".format(user_id))
    # ruleid: pmx-py-sql-injection
    cursor.execute("SELECT * FROM users WHERE name = '" + user_id + "'")
    # ok: pmx-py-sql-injection
    cursor.execute("SELECT * FROM users WHERE id = %s", (user_id,))
    # ok: pmx-py-sql-injection
    print("Hello %s" % user_id)


def loads(blob, text):
    # ruleid: pmx-py-unsafe-deserialization
    pickle.loads(blob)
    # ruleid: pmx-py-unsafe-deserialization
    yaml.load(text)
    # ok: pmx-py-unsafe-deserialization
    yaml.load(text, Loader=yaml.SafeLoader)
    # ok: pmx-py-unsafe-deserialization
    yaml.safe_load(text)


def http(url):
    # ruleid: pmx-py-tls-verification-disabled
    requests.get(url, verify=False)
    # ok: pmx-py-tls-verification-disabled
    requests.get(url, verify="/etc/ssl/ca.pem")


def serve(app):
    # ruleid: pmx-py-debug-enabled
    app.run(host="0.0.0.0", debug=True)
    # ok: pmx-py-debug-enabled
    app.run(host="0.0.0.0")
