# Virtual camera samples

MediaMTX loops these MP4s and exposes them as RTSP streams at
`rtsp://mediamtx:8554/cam-01` and `rtsp://mediamtx:8554/cam-02`.

## What to drop in

Place two H.264 MP4 files here:

- `sample-01.mp4` — served as `cam-01` (mapped to the `entrance` camera row)
- `sample-02.mp4` — served as `cam-02` (mapped to the `dining-area` camera row)

## Requirements

- Container: `.mp4`
- Codec: H.264 (yuv420p). MediaMTX `runOnInit` uses `-c copy`, so any other
  codec will be passed through and may not decode cleanly on the edge worker.
- Resolution: at least 480p (anything higher is fine).
- Length: anything — `-stream_loop -1` loops indefinitely.

## Where to get test footage

- [MOT17 / MOTChallenge](https://motchallenge.net/) — pedestrian tracking clips, public.
- [Pexels — store videos](https://www.pexels.com/search/videos/store/) — CC0 retail clips.
- `yt-dlp` works for any browser-playable clip:

  ```sh
  yt-dlp -f 'best[ext=mp4][height<=720]' -o sample-01.mp4 '<youtube-url>'
  ```

If the file isn't already H.264/yuv420p, transcode once:

```sh
ffmpeg -i input.mov -c:v libx264 -pix_fmt yuv420p -preset veryfast -an sample-01.mp4
```

(The `-an` drops audio — the inference path doesn't use it.)

## Why these aren't committed

Sample MP4s are large binaries and licensing varies per source. The `.gitignore`
at `video-poc/.gitignore` excludes them.
