# Qur's Linux web view: a WebKitGTK window driven by qur_browser (linux.rs).
# Commands arrive on stdin and events leave on stdout, one JSON object per line.
# On Hyprland the window floats exactly over Qur's Browser pane; elsewhere it's
# a normal window beside Qur. Exits when stdin closes (Qur dropped the Browser).
import json
import os
import subprocess
import sys
import warnings

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("WebKit", "6.0")
from gi.repository import Gdk, Gio, GLib, Gtk, WebKit  # noqa: E402

SCRIPT = os.environ.get("QUR_WEB_SCRIPT", "")
HYPR = bool(os.environ.get("HYPRLAND_INSTANCE_SIGNATURE"))
EDIT = {"Copy": "Copy", "Cut": "Cut", "Paste": "Paste", "SelectAll": "SelectAll", "Undo": "Undo", "Redo": "Redo"}


def out(**m):
    sys.stdout.write(json.dumps(m) + "\n")
    sys.stdout.flush()


def hyprctl(*args):
    try:
        return subprocess.run(["hyprctl", *args], capture_output=True, text=True, timeout=2).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


class View:
    def __init__(self, app):
        self.app = app
        self.inspect = None  # accent while inspect mode is on
        self.loading = False
        self.last_nav = None
        self.frame = (0, 0, 800, 600)
        self.visible = False
        self.placed = None

        ucm = WebKit.UserContentManager()
        if SCRIPT:
            with open(SCRIPT, encoding="utf-8") as f:
                src = f.read()
            ucm.add_script(WebKit.UserScript.new(src, WebKit.UserContentInjectedFrames.TOP_FRAME,
                                                 WebKit.UserScriptInjectionTime.END, None, None))
        ucm.register_script_message_handler("qur", None)
        ucm.connect("script-message-received::qur", self.on_message)
        self.wv = WebKit.WebView(user_content_manager=ucm)
        s = self.wv.get_settings()
        s.set_allow_file_access_from_file_urls(True)
        s.set_enable_developer_extras(os.environ.get("QUR_WEB_DEVTOOLS") == "1")
        self.wv.connect("load-changed", self.on_load)
        self.wv.connect("load-failed", self.on_failed)
        self.wv.connect("notify::title", lambda *_: self.nav())
        self.wv.connect("web-process-terminated", self.on_terminated)

        self.win = Gtk.Window(application=app, title="Qur Browser")
        self.win.set_default_size(800, 600)
        if HYPR:
            self.win.set_decorated(False)
        self.win.set_child(self.wv)
        self.win.connect("close-request", self.on_close)
        focus = Gtk.EventControllerFocus()
        focus.connect("enter", lambda *_: out(e="msg", body=json.dumps({"t": "focus"})))
        self.wv.add_controller(focus)
        GLib.timeout_add(700, self.follow)

    # page -> Qur
    def on_message(self, _ucm, value):
        body = value.to_string()
        try:
            m = json.loads(body)
        except ValueError:
            return
        if m.get("t") == "nav":
            # SPA navigations; drop repeats of what load-changed already reported.
            now = (m.get("url", ""), m.get("title", ""))
            if now == self.last_nav:
                return
            self.last_nav = now
        out(e="msg", body=body)

    def set_loading(self, on):
        if self.loading != on:
            self.loading = on
            out(e="loading", on=on)

    def nav(self):
        url = self.wv.get_uri()
        if not url:
            return
        now = (url, self.wv.get_title() or (self.last_nav[1] if self.last_nav and self.last_nav[0] == url else ""))
        if now != self.last_nav:
            self.last_nav = now
            out(e="nav", url=now[0], title=now[1], back=self.wv.can_go_back(), fwd=self.wv.can_go_forward())

    def on_load(self, _wv, ev):
        if ev == WebKit.LoadEvent.STARTED:
            self.set_loading(True)
        elif ev == WebKit.LoadEvent.COMMITTED:
            self.nav()
        elif ev == WebKit.LoadEvent.FINISHED:
            if self.inspect is not None:
                self.eval(inspect_js(True, self.inspect))
            self.nav()
            self.set_loading(False)

    def on_failed(self, _wv, _ev, _uri, err):
        self.set_loading(False)
        # A newer navigation replaced this one, or a download/policy change: not user-facing.
        if err.matches(WebKit.NetworkError.quark(), WebKit.NetworkError.CANCELLED) or \
                err.matches(WebKit.PolicyError.quark(), WebKit.PolicyError.FRAME_LOAD_INTERRUPTED_BY_POLICY_CHANGE):
            return False
        out(e="error", msg=err.message)
        return False

    def on_terminated(self, *_):
        self.set_loading(False)
        out(e="error", msg="The page's web content process stopped.")

    def on_close(self, *_):
        # Closing the window hides it; Qur shows it again with the pane.
        self.win.set_visible(False)
        return True

    def eval(self, js):
        self.wv.evaluate_javascript(js, -1, None, None, None, None, None)

    # Qur -> page
    def run(self, m):
        c = m.get("c")
        if c == "load":
            self.wv.load_uri(m["url"])
        elif c == "reload":
            self.wv.reload()
        elif c == "back":
            self.wv.go_back()
        elif c == "forward":
            self.wv.go_forward()
        elif c == "stop":
            self.wv.stop_loading()
            self.set_loading(False)
        elif c == "frame":
            self.frame = tuple(int(round(v)) for v in m["r"])
            self.place()
        elif c == "visible":
            self.visible = m["on"]
            self.win.set_visible(self.visible)
            self.placed = None
            self.place()
        elif c == "inspect":
            self.inspect = m["accent"] if m["on"] else None
            self.eval(inspect_js(m["on"], m["accent"]))
        elif c == "eval":
            self.eval(m["js"])
        elif c == "snapshot":
            self.snapshot(m["id"], m.get("rect"))
        elif c == "edit":
            self.wv.grab_focus()
            self.wv.execute_editing_command(EDIT[m["cmd"]])
        elif c == "zoom":
            d = m["delta"]
            self.wv.set_zoom_level(1.0 if d == 0 else min(3.0, max(0.3, self.wv.get_zoom_level() + 0.1 * d)))
        elif c == "focus":
            self.win.present()
            self.wv.grab_focus()

    def snapshot(self, sid, rect):
        def done(wv, res):
            png = None
            try:
                tex = wv.get_snapshot_finish(res)
                if rect:
                    k = tex.get_width() / max(1, wv.get_width()) * wv.get_zoom_level()
                    x0, y0 = max(0, int(rect[0] * k)), max(0, int(rect[1] * k))
                    x1, y1 = min(tex.get_width(), int((rect[0] + rect[2]) * k)), min(tex.get_height(), int((rect[1] + rect[3]) * k))
                    if x1 - x0 >= 1 and y1 - y0 >= 1:
                        # GdkPixbuf is the only cropper PyGObject has for a texture.
                        warnings.simplefilter("ignore", DeprecationWarning)
                        pb = Gdk.pixbuf_get_from_texture(tex).new_subpixbuf(x0, y0, x1 - x0, y1 - y0)
                        ok, buf = pb.save_to_bufferv("png", [], [])
                        png = bytes(buf).hex() if ok else None
                else:
                    png = tex.save_to_png_bytes().get_data().hex()
            except GLib.Error:
                pass
            out(e="snap", id=sid, png=png)

        self.wv.get_snapshot(WebKit.SnapshotRegion.VISIBLE, WebKit.SnapshotOptions.NONE, None, done)

    # Hyprland: float over Qur's pane, on Qur's workspace.
    def place(self):
        if not (HYPR and self.visible):
            if not HYPR:
                self.win.set_default_size(max(200, self.frame[2]), max(150, self.frame[3]))
            return
        try:
            clients = json.loads(hyprctl("clients", "-j") or "[]")
        except ValueError:
            return
        qur = next((c for c in clients if c.get("pid") == os.getppid() and c.get("mapped")), None)
        me = next((c for c in clients if c.get("pid") == os.getpid()), None)
        if not qur or not me:
            return
        x, y, w, h = self.frame
        want = (qur["at"][0] + x, qur["at"][1] + y, max(1, w), max(1, h), qur["workspace"]["id"])
        if want == self.placed:
            return
        self.placed = want
        p = f"pid:{os.getpid()}"
        cmds = [] if me.get("floating") else [f"dispatch setfloating {p}"]
        if me["workspace"]["id"] != want[4]:
            cmds.append(f"dispatch movetoworkspacesilent {want[4]},{p}")
        cmds += [f"dispatch resizewindowpixel exact {want[2]} {want[3]},{p}",
                 f"dispatch movewindowpixel exact {want[0]} {want[1]},{p}"]
        hyprctl("--batch", " ; ".join(cmds))

    def follow(self):
        # Qur's window can move without its pane changing size.
        if self.visible:
            self.place()
        return True


def unix_stdin():
    try:
        gi.require_version("GioUnix", "2.0")
        from gi.repository import GioUnix
        return GioUnix.InputStream.new(0, False)
    except (ValueError, ImportError):
        return Gio.UnixInputStream.new(0, False)


def inspect_js(on, accent):
    return f"window.__qur && window.__qur.inspect({'true' if on else 'false'}, {json.dumps(accent)});"


def main():
    app = Gtk.Application(application_id="dev.qur.Browser", flags=Gio.ApplicationFlags.NON_UNIQUE)
    state = {}

    def activate(app):
        app.hold()
        view = state["view"] = View(app)
        stream = Gio.DataInputStream.new(unix_stdin())

        def got(s, res):
            try:
                line, _ = s.read_line_finish_utf8(res)
            except GLib.Error:
                line = None
            if line is None:
                app.quit()
                return
            try:
                view.run(json.loads(line))
            except (ValueError, KeyError) as e:
                out(e="error", msg=f"web view: {e}")
            s.read_line_async(GLib.PRIORITY_DEFAULT, None, got)

        stream.read_line_async(GLib.PRIORITY_DEFAULT, None, got)
        out(e="ready")

    app.connect("activate", activate)
    app.run([])


if __name__ == "__main__":
    main()
