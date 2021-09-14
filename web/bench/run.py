"""

python3 -m pip install --user playwright matplotlib wget numpy
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

PORT = 8000 # random.randint(8000, 9000)

def serve():
    with socketserver.TCPServer(("", PORT), http.server.SimpleHTTPRequestHandler) as httpd:
        #print("serving at port", PORT)
        httpd.serve_forever()

#t = threading.Thread(target=serve, daemon=True)
#t.start()
#time.sleep(1)

started = False
finished = False
cur_loop = None

def print_args(msg):
    global finished
    for arg in msg.args:
         s = str(arg.json_value())
         if s.startswith("run_frame() took "):
            #print(s)
            if started and not finished:
                data[cur_loop].append(float(s[17:-3]))
                if len(data[cur_loop]) > 100:
                    #print("finished")
                    finished = True

loops = [
    #37,

    3664,
    4449,
    7081,

    4145,

    2379,
    5092,
    7214,
    7311,
    3946,
    1650,

    #437,


    #1958,
    #4023,

    #7711,

    #2139,
]

data = {}
for l in loops:
   data[l] = []

with sync_playwright() as p:
    for l in loops:
        cur_loop = l

        if not os.path.exists("z0r-de_" + str(l) + ".swf"):
            url = "https://z0r.de/L/z0r-de_" + str(l) + ".swf"
            #print("Downloading", url)
            wget.download(url)

        browser = p.chromium.launch(headless=False) #, firefox_user_prefs= {"webgl.force-enabled": True})

        started = False
        finished = False

        page = browser.new_page()
        page.on("console", print_args)
        page.goto("http://localhost:" + str(PORT) + "/?loop=" + str(l))
        page.wait_for_timeout(1000)
        page.click("#play_button")
        started = True
        while not finished:
            page.wait_for_timeout(1000)

        browser.close()

#print("AVG run_frame duration for loops (excluding the first few frames):")
for d in data:
    plt.plot(data[d], label=d)

    avg = np.mean(data[d][20:])

    #print("   ", d, ":", round(avg, 4), "\t# in ms,", str(round(speedup * 100.0, 2)) + "%% of baseline")
    print(d, "_avgtime:", round(avg, 4))

print("mean : ", np.mean(list(data.values()))) # this only works because each element is of same length
#print("(excluding the first few frames)")

"""
plt.ylim(bottom=0)
plt.grid()
plt.title("Duration of run_frame in some loops")
plt.xlabel("frame number")
plt.ylabel("run_frame duration [ms]")
plt.legend()
plt.show()
"""
