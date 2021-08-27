"""

python3 -m pip install --user playwright matplotlib wget
python3 -m playwright install
python3 ./run.py

"""


import os
import time
import threading
import random

import http.server
import socketserver

import wget
import numpy as np
import matplotlib.pyplot as plt
from playwright.sync_api import sync_playwright

PORT = random.randint(8000, 9000)

def serve():
    with socketserver.TCPServer(("", PORT), http.server.SimpleHTTPRequestHandler) as httpd:
        print("serving at port", PORT)
        httpd.serve_forever()

t = threading.Thread(target=serve, daemon=True)
t.start()
time.sleep(1)

started = False
finished = False
cur_loop = None

def print_args(msg):
    global finished
    for arg in msg.args:
         s = str(arg.json_value())
         if s.startswith("run_frame() took "):
            print(s)
            if started and not finished:
                data[cur_loop].append(float(s[17:-3]))
                if len(data[cur_loop]) > 100:
                    print("finished")
                    finished = True


loops = ["37", "7311", "4145", "437", "1650", "2139", "4023", "3664", "3946", "4449", "7081", "7711"]
data = {}
for l in loops:
   data[l] = []

with sync_playwright() as p:
    # TODO: unset MOZ_X11_EGL from env ?
    for l in loops:
        cur_loop = l

        if not os.path.exists("z0r-de_" + l + ".swf"):
            wget.download("https://z0r.de/L/z0r-de_" + l + ".swf")

        browser = p.chromium.launch(headless=False) #, firefox_user_prefs= {"webgl.force-enabled": True})

        started = False
        finished = False

        page = browser.new_page()
        page.on("console", print_args)
        page.goto("http://localhost:" + str(PORT) + "/?loop=" + l)
        page.wait_for_timeout(1000)
        page.click("#play_button")
        started = True
        while not finished:
            page.wait_for_timeout(1000)

        browser.close()


for d in data:
    plt.plot(data[d], label=d)
    print("AVG run_frame duration for loop", d, "=", np.mean(data[d][10:]), "ms")
print("(excluding the first few frames)")

plt.ylim(bottom=0)
plt.grid()
plt.title("Duration of run_frame in some loops")
plt.xlabel("frame number")
plt.ylabel("run_frame duration [ms]")
plt.legend()
plt.show()
