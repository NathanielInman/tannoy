#!/usr/bin/env python3
"""A fake MPRIS player for trying Tannoy without touching real media.

    tools/fake-player.py --name tannoyfake --title "One More Time" \
        --artist "Daft Punk" --album Discovery --length 320 --status Playing

It owns org.mpris.MediaPlayer2.<name>, answers the Player methods (play,
pause, next, previous, seek, set position) and emits PropertiesChanged and
Seeked like a real player. Next and Previous step through --title values
when several are given. Needs python-gobject.
"""

import argparse
import time

from gi.repository import Gio, GLib

XML = """
<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/><method name="Quit"/>
    <property name="Identity" type="s" access="read"/>
    <property name="DesktopEntry" type="s" access="read"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
    <property name="HasTrackList" type="b" access="read"/>
    <property name="SupportedUriSchemes" type="as" access="read"/>
    <property name="SupportedMimeTypes" type="as" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Next"/><method name="Previous"/><method name="Pause"/>
    <method name="PlayPause"/><method name="Stop"/><method name="Play"/>
    <method name="Seek"><arg direction="in" name="Offset" type="x"/></method>
    <method name="SetPosition">
      <arg direction="in" name="TrackId" type="o"/>
      <arg direction="in" name="Position" type="x"/>
    </method>
    <signal name="Seeked"><arg name="Position" type="x"/></signal>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="Rate" type="d" access="read"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="Volume" type="d" access="read"/>
    <property name="Position" type="x" access="read"/>
    <property name="MinimumRate" type="d" access="read"/>
    <property name="MaximumRate" type="d" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
</node>
"""

PATH = "/org/mpris/MediaPlayer2"


class Fake:
    def __init__(self, args):
        self.a = args
        self.index = 0
        self.status = args.status
        self.pos = 0  # microseconds at self.at
        self.at = time.monotonic()
        self.conn = None

    def position(self):
        p = self.pos
        if self.status == "Playing":
            p += int((time.monotonic() - self.at) * 1e6)
        return min(p, self.a.length * 1_000_000)

    def set_pos(self, us):
        self.pos = max(0, min(us, self.a.length * 1_000_000))
        self.at = time.monotonic()

    def metadata(self):
        title = self.a.title[self.index % len(self.a.title)]
        md = {
            "mpris:trackid": GLib.Variant("o", f"{self.a.trackid}{self.index}"),
            "mpris:length": GLib.Variant("x", self.a.length * 1_000_000),
            "xesam:title": GLib.Variant("s", title),
            "xesam:artist": GLib.Variant("as", self.a.artist),
            "xesam:album": GLib.Variant("s", self.a.album),
        }
        if self.a.art:
            md["mpris:artUrl"] = GLib.Variant("s", self.a.art[self.index % len(self.a.art)])
        if self.a.genre:
            md["xesam:genre"] = GLib.Variant("as", self.a.genre)
        return md

    def prop(self, iface, name):
        root = {
            "Identity": GLib.Variant("s", self.a.identity),
            "DesktopEntry": GLib.Variant("s", self.a.desktop_entry),
            "CanQuit": GLib.Variant("b", False),
            "CanRaise": GLib.Variant("b", self.a.can_raise),
            "HasTrackList": GLib.Variant("b", False),
            "SupportedUriSchemes": GLib.Variant("as", []),
            "SupportedMimeTypes": GLib.Variant("as", []),
        }
        player = {
            "PlaybackStatus": GLib.Variant("s", self.status),
            "Rate": GLib.Variant("d", 1.0),
            "Metadata": GLib.Variant("a{sv}", self.metadata()),
            "Volume": GLib.Variant("d", 1.0),
            "Position": GLib.Variant("x", self.position()),
            "MinimumRate": GLib.Variant("d", 1.0),
            "MaximumRate": GLib.Variant("d", 1.0),
            "CanGoNext": GLib.Variant("b", len(self.a.title) > 1),
            "CanGoPrevious": GLib.Variant("b", len(self.a.title) > 1),
            "CanPlay": GLib.Variant("b", True),
            "CanPause": GLib.Variant("b", True),
            "CanSeek": GLib.Variant("b", True),
            "CanControl": GLib.Variant("b", True),
        }
        table = root if iface == "org.mpris.MediaPlayer2" else player
        return table.get(name)

    def changed(self, names):
        iface = "org.mpris.MediaPlayer2.Player"
        body = {n: self.prop(iface, n) for n in names}
        self.conn.emit_signal(
            None, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged",
            GLib.Variant("(sa{sv}as)", (iface, body, [])),
        )

    def seeked(self):
        self.conn.emit_signal(
            None, PATH, "org.mpris.MediaPlayer2.Player", "Seeked",
            GLib.Variant("(x)", (self.position(),)),
        )

    def call(self, conn, sender, path, iface, method, params, inv):
        if method in ("Play", "Pause", "PlayPause", "Stop"):
            p = self.position()
            if method == "PlayPause":
                self.status = "Paused" if self.status == "Playing" else "Playing"
            else:
                self.status = {"Play": "Playing", "Pause": "Paused", "Stop": "Stopped"}[method]
            self.set_pos(p)
            self.changed(["PlaybackStatus"])
        elif method in ("Next", "Previous"):
            self.index += 1 if method == "Next" else -1
            self.set_pos(0)
            self.changed(["Metadata", "CanGoNext", "CanGoPrevious"])
        elif method == "Seek":
            self.set_pos(self.position() + params.unpack()[0])
            self.seeked()
        elif method == "SetPosition":
            self.set_pos(params.unpack()[1])
            self.seeked()
        print(f"{method} {params.unpack() if params else ''}", flush=True)
        inv.return_value(None)

    def get(self, conn, sender, path, iface, name):
        return self.prop(iface, name)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--name", default="tannoyfake")
    ap.add_argument("--identity", default="Tannoy Fake")
    ap.add_argument("--desktop-entry", default="")
    ap.add_argument("--title", action="append")
    ap.add_argument("--artist", action="append")
    ap.add_argument("--album", default="")
    ap.add_argument("--genre", action="append")
    ap.add_argument("--art", action="append", help="mpris:artUrl, one per title")
    ap.add_argument("--length", type=int, default=240, help="seconds")
    ap.add_argument("--trackid", default="/org/tannoy/fake/track")
    ap.add_argument("--can-raise", action="store_true", help="answer Raise")
    ap.add_argument("--status", default="Playing", choices=["Playing", "Paused", "Stopped"])
    args = ap.parse_args()
    args.title = args.title or ["Fake Track"]
    args.artist = args.artist or ["Fake Artist"]

    fake = Fake(args)
    node = Gio.DBusNodeInfo.new_for_xml(XML)

    def on_bus(conn, name):
        fake.conn = conn
        for iface in node.interfaces:
            conn.register_object(PATH, iface, fake.call, fake.get, None)

    Gio.bus_own_name(
        Gio.BusType.SESSION, f"org.mpris.MediaPlayer2.{args.name}",
        Gio.BusNameOwnerFlags.NONE, on_bus, None, None,
    )
    GLib.MainLoop().run()


if __name__ == "__main__":
    main()
