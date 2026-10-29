# Troubleshooting

Run the doctor first. It usually tells you exactly what to fix:

- Windows: `powershell -ExecutionPolicy Bypass -File .\windows\doctor.ps1`
- Mac: `./mac/doctor.sh`

## Mac → Windows (Windows App / RDP)

### "Unable to connect", error 0x204 or a timeout
The Mac can't reach port 3389.
1. **Tailscale must be connected on both machines with the same account.** Check `tailscale status` on each; the other machine should be listed and online.
2. **Use the Tailscale name (`it`) or the 100.x IP, not the 192.168.x.x Wi-Fi IP.** LAN IPs fail on different networks, and on Wi-Fi with "client isolation" (common on office and guest networks).
3. **macOS Local Network permission.** Go to System Settings → Privacy & Security → Local Network and turn on **Windows App**. If it's off, LAN connections fail silently.
4. Check the Windows PC is awake. With the lid closed on battery, it sleeps.
5. Run `nc -z it 3389` from the Mac. If that succeeds, the problem is inside Windows App, not the network.

### Login rejected / "credentials did not work" / 0x907
This PC signs in with a **Microsoft account**.
- Username: `MicrosoftAccount\you@outlook.com`, or just the email.
- Password: the **Microsoft account password**. The Windows Hello **PIN does not work** over RDP.
- If you have only ever used the PIN on this PC, Windows has no cached password. Lock the PC (Win+L), choose *Sign-in options → password*, and sign in once with the password. Then retry.
- Settings → Accounts → Sign-in options → turn off **"For improved security, only allow Windows Hello sign-in"**.
- If you changed the Microsoft password recently, repeat the password sign-in at the console.
- In Windows App, delete any saved credential for this PC and enter it fresh.

### Connects, then drops or goes black
- Keep the PC **plugged in**. `setup.ps1` sets lid-close = do nothing and sleep = never, but only on AC.
- The generated `.rdp` file already turns off persistent bitmap caching, which is the usual cause of a black screen. If you get a black screen anyway, press `Ctrl+Alt+End`, or disconnect and reconnect.
- If drops continue on a weak network, turn off UDP transport on the Windows side (as admin), then reboot:
  `reg add "HKLM\SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services" /v SelectTransport /t REG_DWORD /d 1 /f`
  To undo it: `reg delete ... /v SelectTransport`.
- Someone signing in locally at the PC ends your remote session. That's normal on Windows client editions, which allow one session at a time.

## Windows → Mac (TigerVNC / Screen Sharing)

### "Connection refused" / unreachable
- Turn on Mac System Settings → General → Sharing → **Screen Sharing**.
- Make sure Tailscale is running on the Mac and the Mac is awake and on its charger.

### Authentication fails
TigerVNC supports Apple's login (Mac username + password) on most macOS versions. If it fails:
1. Go to Sharing → Screen Sharing → (i) and turn on **"VNC viewers may control screen with password"**, then set a password of up to 8 characters.
2. Connect again and enter only that password.

### Screen is view-only or black
Go to Sharing → Screen Sharing → (i) → **Anyone may request permission** / your user must have "Control" access. On the login window, a black screen is normal until you type the password.

## SSH

- `ssh win` (from the Mac) and `ssh mac` (from Windows) use the aliases in `~/.ssh/config`.
- Windows: `green` is an admin, so the Mac's key must be in `C:\ProgramData\ssh\administrators_authorized_keys`, not in `~\.ssh\authorized_keys`. Re-run `setup.ps1 -MacPublicKey "..."` to add it.
- Mac: if Remote Login can't be enabled from Terminal, give Terminal Full Disk Access or turn it on in System Settings.

## Logs
Each `windows\setup.ps1` run writes a transcript to `logs\` (git-ignored).
