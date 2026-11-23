#!/usr/bin/env python3
"""Exercise only a private, offline Tor child; never configure a system service."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def terminal_smoke(binary, socket, expected_pid, preferences_directory, measure_seconds=0):
    """Drive a real terminal, including resize and restoration on quit/SIGTERM."""
    import fcntl
    import pty
    import select
    import signal
    import struct
    import termios

    for shutdown in ("q", "SIGTERM"):
        master, slave = pty.openpty()
        original = termios.tcgetattr(slave)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        env = os.environ.copy()
        env["TERM"] = "xterm-256color"
        env["XDG_CONFIG_HOME"] = str(preferences_directory)

        def controlling_terminal():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        process = subprocess.Popen([str(binary), "--socket", str(socket), "--ascii", "--no-color"],
                                   stdin=slave, stdout=slave, stderr=slave, env=env,
                                   preexec_fn=controlling_terminal)
        received = bytearray()
        try:
            deadline = time.monotonic() + 5
            # The compact header intentionally omits normal connection status.
            marker = str(expected_pid).encode()
            while marker not in received and time.monotonic() < deadline:
                if select.select([master], [], [], 0.1)[0]:
                    received.extend(os.read(master, 65536))
                if process.poll() is not None:
                    raise RuntimeError("TUI exited before connecting")
            if marker not in received:
                raise RuntimeError("TUI never displayed a real control connection")
            if shutdown == "q" and measure_seconds > 0:
                def cpu_ticks():
                    fields = Path(f"/proc/{process.pid}/stat").read_text().rsplit(")", 1)[1].split()
                    return int(fields[11]) + int(fields[12])

                ticks_before = cpu_ticks()
                started = time.monotonic()
                rss = 0
                while time.monotonic() - started < measure_seconds:
                    if select.select([master], [], [], 0.1)[0]:
                        received.extend(os.read(master, 65536))
                    for row in Path(f"/proc/{process.pid}/status").read_text().splitlines():
                        if row.startswith("VmRSS:"):
                            rss = max(rss, int(row.split()[1]))
                elapsed = time.monotonic() - started
                cpu = (cpu_ticks() - ticks_before) / os.sysconf("SC_CLK_TCK")
                print(f"Monitor with offline idle Tor: {elapsed:.2f}s, {100 * cpu / elapsed:.2f}% of one CPU, max sampled RSS {rss} KiB", flush=True)
            if shutdown == "q":
                os.write(master, b"p")
                time.sleep(0.2)
                while select.select([master], [], [], 0)[0]:
                    received.extend(os.read(master, 65536))
                if b"PAUSED" not in received:
                    raise RuntimeError("Pause indicator was not displayed")
                paused_bytes = len(received)
                deadline = time.monotonic() + 1.2
                while time.monotonic() < deadline:
                    if select.select([master], [], [], 0.1)[0]:
                        received.extend(os.read(master, 65536))
                if len(received) != paused_bytes:
                    raise RuntimeError("Paused display emitted an unsolicited terminal redraw")
                os.write(master, b"p")
                # 'q' inside the search editor must never quit the application.
                keys = (b"2", b"/", b"q", b"\r", b"\x1b", b"e", b"d",
                        b"3", b"/", b"neighbor", b"\r", b"s", b"r", b"\x1b",
                        b"1", b"+", b"+", b"-", b"-", b"]", b"-", b"h", b"1")
            else:
                settings = (preferences_directory / "tor-relay-monitor/ui.toml").read_text()
                if "graph_height = 13" not in settings or "history_seconds = 90" not in settings:
                    raise RuntimeError("Display preferences did not survive a restart")
                keys = (b"2", b"3", b"h", b"1")
            for key in keys:
                os.write(master, key)
                time.sleep(0.05)
                while select.select([master], [], [], 0)[0]:
                    received.extend(os.read(master, 65536))
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 18, 60, 0, 0))
            process.send_signal(signal.SIGWINCH)
            time.sleep(0.1)
            if shutdown == "q":
                os.write(master, b"q")
            else:
                process.terminate()
            process.wait(timeout=5)
            while select.select([master], [], [], 0)[0]:
                received.extend(os.read(master, 65536))
            if process.returncode != 0:
                raise RuntimeError(f"TUI failed during {shutdown}: {process.returncode}")
            if termios.tcgetattr(slave) != original:
                raise RuntimeError(f"Terminal attributes were not restored after {shutdown}")
            if b"\x1b[?1049l" not in received:
                raise RuntimeError("Alternate screen was not restored")
            print(f"Real TUI navigation, resize and terminal restoration: {shutdown} passed", flush=True)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            os.close(master)
            os.close(slave)


def quote(path):
    return '"' + str(path).replace("\\", "/").replace('"', '\\"') + '"'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tor", default="tor", help="Existing Tor executable (never installed by this script)")
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--release", action="store_true", help="Validate optimized binaries")
    parser.add_argument("--accounting", action="store_true", help="Enable a daily accounting period only in the private offline Tor")
    parser.add_argument("--measure-seconds", type=float, default=0, help="Measure monitor idle CPU/RSS on Linux during the terminal test")
    args = parser.parse_args()
    if not 0 <= args.measure_seconds <= 60:
        parser.error("--measure-seconds must be in 0..60")
    profile = ["--release"] if args.release else []
    tor = shutil.which(args.tor)
    if not tor:
        parser.error("Tor executable not found; supply --tor PATH")
    with tempfile.TemporaryDirectory(prefix="trm-test-") as directory:
        root = Path(directory)
        empty = root / "defaults"
        empty.write_text("", encoding="utf-8")
        port_file = root / "control-port"
        cookie = root / "cookie"
        socket = root / "control"
        lines = [
            f"DataDirectory {quote(root / 'data')}",
            "SocksPort 0", "DisableNetwork 1", "ClientOnly 1", "RunAsDaemon 0",
            "CookieAuthentication 1", f"CookieAuthFile {quote(cookie)}",
            "ControlPort auto", f"ControlPortWriteToFile {quote(port_file)}",
            "AvoidDiskWrites 1", "Log notice stdout",
        ]
        if os.name == "posix":
            lines.append(f"ControlSocket {quote(socket)}")
        if args.accounting:
            lines.extend(["AccountingMax 1 GBytes", "AccountingStart day 00:00"])
        torrc = root / "torrc"
        torrc.write_text("\n".join(lines) + "\n", encoding="utf-8")
        # Child output is kept only in this private temporary directory for failure diagnosis.
        with (root / "startup.txt").open("w+", encoding="utf-8") as output:
            process = subprocess.Popen([tor, "--defaults-torrc", str(empty), "-f", str(torrc)],
                                       stdout=output, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 20
                while not (port_file.exists() and cookie.exists()):
                    if process.poll() is not None or time.monotonic() > deadline:
                        output.seek(0)
                        raise RuntimeError("Private Tor did not start:\n" + output.read())
                    time.sleep(0.05)
                address = next(line.removeprefix("PORT=") for line in
                               port_file.read_text(encoding="utf-8").splitlines()
                               if line.startswith("PORT="))
                environments = [("TCP", {"TRM_TEST_TCP": address})]
                if os.name == "posix":
                    environments.append(("Unix", {"TRM_TEST_SOCKET": str(socket)}))
                for label, selected in environments:
                    env = os.environ.copy()
                    for key in ("TRM_TEST_TCP", "TRM_TEST_SOCKET", "TRM_TEST_COOKIE", "TRM_TEST_ACCOUNTING", "TRM_TEST_RESOURCES"):
                        env.pop(key, None)
                    env.update(selected)
                    if args.accounting:
                        env["TRM_TEST_ACCOUNTING"] = "1"
                    if Path("/proc/self/stat").is_file():
                        env["TRM_TEST_RESOURCES"] = "1"
                    print(f"Testing private offline Tor over {label}", flush=True)
                    subprocess.run([args.cargo, "test", *profile, "--locked", "--test", "control", "real_local_tor", "--", "--ignored", "--nocapture"], env=env, check=True, timeout=120)
                    endpoint = ["--tcp", address] if label == "TCP" else ["--socket", str(socket)]
                    subprocess.run([args.cargo, "run", *profile, "--bin", "trm", "--locked", "--", "--diagnose", *endpoint], env=env, check=True, timeout=120)
                if os.name == "posix":
                    target = Path(os.environ.get("CARGO_TARGET_DIR", "target")).resolve()
                    terminal_smoke(target / ("release" if args.release else "debug") / "trm", socket, process.pid, root / "preferences", args.measure_seconds)
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)


if __name__ == "__main__":
    main()
