# Third-party notices

MediaFilez Desktop source code is licensed under the MIT License. The programs below are separate works with their own licenses.

## yt-dlp

MediaFilez Desktop can locate a system installation or download an official standalone release from <https://github.com/yt-dlp/yt-dlp/releases>.

yt-dlp source code is generally Unlicense, but official standalone executables combine dependencies and are distributed under GPL-3.0-or-later. Refer to the release and repository notices for the exact binary. MediaFilez Desktop verifies the release SHA-256 but does not change its license.

## gallery-dl

MediaFilez Desktop can locate a system installation or download the official Windows standalone executable from <https://codeberg.org/mikf/gallery-dl/releases>. gallery-dl is licensed under GPL-2.0-only. The app verifies the release SHA-256 before installation.

## Deno

MediaFilez Desktop can locate a system installation or download an official archive from <https://github.com/denoland/deno/releases>. Deno is licensed under the MIT License. Its runtime lets yt-dlp solve JavaScript challenges required by current YouTube extractors.

## FFmpeg

On Windows, MediaFilez Desktop can download the FFmpeg essentials archive published at <https://www.gyan.dev/ffmpeg/builds/>. FFmpeg and the build's enabled libraries determine whether LGPL or GPL terms apply. License and corresponding-source links are published with the build.

MediaFilez Desktop does not bundle these tools in its source tree. Anyone redistributing an installer with the tools prepackaged must review and satisfy their licenses independently.
