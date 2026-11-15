import ctypes
from ctypes import wintypes as W

u = ctypes.windll.user32
u.SetProcessDPIAware()
h = None
while True:
    h = u.FindWindowExW(None, h, "NovaWidget", None)
    if not h:
        break
    r = W.RECT()
    u.GetWindowRect(h, ctypes.byref(r))
    top = bool(u.GetWindowLongW(h, -20) & 8)
    print(r.left, r.top, "topmost" if top else "bottom")
