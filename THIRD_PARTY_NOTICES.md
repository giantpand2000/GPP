# Third-party notices

GPP's macOS release bundles the official GStreamer runtime from the
[GStreamer project](https://gstreamer.freedesktop.org/). GStreamer is
distributed under the GNU Lesser General Public License, version 2.1 or later.
A copy of version 2.1 is included in `Licenses/LGPL-2.1.txt` inside the app.

The runtime contains GStreamer plug-ins and supporting codec libraries whose
licenses vary by component, including components distributed under the GNU
General Public License version 2 or later. Copies of GPL version 2 and LGPL
version 2.1 are included in the `Licenses` directory. Copyright and license
metadata for an installed plug-in can be inspected with `gst-inspect-1.0`.
Corresponding source code is available from the
[GStreamer source downloads](https://gstreamer.freedesktop.org/src/) and the
upstream projects referenced by each plug-in.

GPP dynamically links to these libraries and does not modify them. The bundled
runtime can be replaced with a compatible build by replacing
`GPP.app/Contents/Frameworks/GStreamer.framework` and re-signing the app bundle.

GStreamer and its plug-ins are independent projects; their inclusion does not
imply endorsement of GPP by their authors.
