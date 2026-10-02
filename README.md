# IPTelly

IPTelly is a fast and powerful IPTV player for desktop, carefully crafted to deliver the best IPTV experience. It's a fork of [open-tv](https://github.com/Fredolx/open-tv), rebranded and maintained independently.

*(Screenshot coming soon.)*

## Features:
- Import your IPTV channels from any source (M3U File, M3U link, Xtream) 🗃️
- Record while watching 🎥
- Multi IPTV sources 🎊
- Control the UI from a TV remote 📺
- Super low RAM usage, crazy speeds, and instant search 🚅
- Refresh your sources when you need it 🔄
- Add channels to favorites 🌟
- Make your own custom channels
- Share your custom channels with friends
- Re-stream channels to friends or other devices (phone, tv)

## Prerequisites
If you are on Windows or use the flatpak on Linux; SKIP THIS PART. 

The app depends on mpv, ffmpeg and yt-dlp. 
If you are on MacOS, you must use Brew or MacPorts to install those dependencies. 

On Fedora, you must add rpmfusion to install those packages.

On Debian or LTS distro, I would strongly suggest using a backport for yt-dlp.

The Windows build **comes with mpv included** (.msi), but you can still install mpv from a package manager of your choice to always have the latest version installed

```
brew install mpv ffmpeg yt-dlp #MacOS
sudo dnf install mpv ffmpeg yt-dlp #Fedora
sudo zypper install mpv ffmpeg yt-dlp #OpenSUSE
sudo pacman -Syu mpv ffmpeg yt-dlp #Arch
sudo apt install mpv ffmpeg yt-dlp #Debian/Ubuntu
scoop install mpv ffmpeg yt-dlp # Windows
choco install mpv ffmpeg yt-dlp # Windows alternative
```

## Docker
You can install IPTelly using Docker. It wouldn't necessarily be the recommended way to install it, but it's now possible.
You should always install IPTelly from either Releases or Flatpak on Linux.

For Nvidia GPUs:
```
docker run --rm -it \
  --net=host \
  --env="DISPLAY" \
  --volume="/tmp/.X11-unix:/tmp/.X11-unix:rw" \
  --volume="$HOME/.Xauthority:/root/.Xauthority:rw" \
  --volume="$HOME/.local/share/dev.iptelly.iptelly:/root/.local/share/dev.iptelly.iptelly" \
  --gpus all \
  ghcr.io/iptelly/iptelly:latest
```
For everyone else (Intel, AMD):
```
docker run --rm -it \
  --net=host \
  --env="DISPLAY" \
  --volume="/tmp/.X11-unix:/tmp/.X11-unix:rw" \
  --volume="$HOME/.Xauthority:/root/.Xauthority:rw" \
  --volume="$HOME/.local/share/dev.iptelly.iptelly:/root/.local/share/dev.iptelly.iptelly" \
  --device /dev/dri \
  ghcr.io/iptelly/iptelly:latest
```

## Development
Want to build IPTelly from source or contribute code? See [DEVELOPMENT.md](DEVELOPMENT.md)
for how to check out the source, run it in dev mode, and build it locally.

## Feedback
Feel free to submit any kind of feedback by creating a new issue.

## Hotkeys
* F1: Help
* Ctrl + a: Show all channels
* Ctrl + s: Show categories
* Ctrl + d: Show favorites
* Ctrl + f: Search
* Ctrl + q: Enable/Disable livestreams
* Ctrl + w: Enable/Disable movies
* Ctrl + e: Enable/Disable series
* Backspace/Esc: Go back
* Arrow keys/Tab/Shift+Tab: Navigation

If you have a tv remote or air mouse that has slightly different bindings for general nav (back, up, down, left, right),
please open an issue and we will add them if it's feasible. Otherwise, you can still use hwdb to make them match IPTelly's bindings.

## Settings explained

**Stream Caching**

Why enabling:
  - If you have a slow internet connection/IPTV provider causing the stream to pause often

Why disabling: 
  - If the stream often drops completely. It will prevent the stream from jumping too far ahead/behind
  - If you have a good internet/provider and want lower latency
  - Can prevent some weird bugs/slowdowns

## Disclaimer

IPTelly is an independent open-source project created to provide a fast and powerful IPTV experience. The name "IPTelly" is used solely to represent this specific software and its purpose as described in the project documentation. Any other software, applications, or products bearing the same or similar name are unrelated to this project. Any resemblance to other software or applications is purely coincidental and unintended. We do not intend to cause confusion or imply affiliation with any other products or organizations that may share a similar name.

IPTelly is licensed under GPLv2, same as the [open-tv](https://github.com/Fredolx/open-tv) project it was forked from.
