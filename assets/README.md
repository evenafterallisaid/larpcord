# larpcord branding

the user selected the glass half-mask mascot and wordmark from [this shared design](https://chatgpt.com/s/m_6af5e32be8808191b002af94fa3ea8e0). the original download is preserved as `brand-source.png`.

`icon.png` is a mascot-only app-icon treatment prepared with the built-in image generation tool. `mascot.png`, `icon.ico` and `Larpcord.icns` are format and size conversions for the native ui and desktop packages. the source wordmark remains available in the original image.

the preparation prompt was

> edit target: the supplied user-selected larpcord logo. prepare a square desktop app icon using only the mascot at the left of the supplied design. faithfully preserve the exact original compact glass speech-bubble/controller silhouette, two rounded top bumps, left oval eye and smile, and the white smiling theatrical half-mask covering the right side. preserve the blue-violet liquid glass and white highlights from the original. remove all lettering. center the same mascot, at about 78% canvas width, on a perfectly clean solid very dark charcoal background #1e1f22. keep all contours crisp and smooth with clean edges. background must be fully opaque. no floating fragments, dots, glow particles, extra objects, or redesign of the source mascot. the source design is already approved. only the icon framing and backdrop are changing.

for the in-app mark, a second built-in edit removed the backdrop. the prompt asked to preserve the exact mascot and its colors, keep the full face opaque, and make only the pixels outside its silhouette transparent. the output's alpha channel was checked before resizing to `mascot.png`. the desktop icon keeps its separate dark app-icon plate.

`home.svg` is a separate, hand-drawn flat mask mark for the Home/DM button. it uses one solid foreground color and transparent cutouts, with no gradients, shadows or raster texture. the app and titlebar keep their colored mascot.
