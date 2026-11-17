# Amboss – Map editor for Source 1 / Garry's Mod

**Version 0.9.10** – shown next to the logo, in the window title and under Help → About Amboss. See `CHANGELOG.md` for what's new (the changelog is in German).

Amboss is a standalone level editor written in **Rust**, inspired by Hammer/Hammer++.
It edits **VMF files**, shows the real **textures and models from the GMod/HL2 VPKs** and
compiles the map with Valve's original tools (**VBSP / VVIS / VRAD**). Afterwards it
launches Garry's Mod straight into the new map.

The user interface is available in **English and German** (Settings → Graphics → Language).

![Built with](https://img.shields.io/badge/Rust-egui%20%2B%20OpenGL-orange) ![License](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-blue)

## Download

Ready-to-run Windows builds are on the [Releases page](https://github.com/Deadshotlp/Amboss/releases).
Unzip anywhere and start `amboss.exe` – nothing gets installed.

> Amboss is under active development. It has mostly been tested with automated tests and a
> simulated game setup; please report problems with real Garry's Mod installations as issues.

## Features

### Getting started
* **Setup assistant** on first launch (Help → Setup assistant…): language, interface mode, find Garry's Mod automatically or pick it (checks gameinfo.txt, compilers, game executable, FGD), content from addons/Workshop and mounted games (mount.cfg).
* **Tutorial** (Help → Tutorial…) in nine chapters – first map, textures and shapes, entities, doors and triggers, compiling and testing, plus terrain with displacements, light/sky/fog, detail work (vertex, 3-point clip, overlays, decals) and building faster (prefabs, generators, walk mode, testing in game). Every step explains the task, highlights the right tool, checks by itself whether it's done, and can demonstrate it with "Show me".
* **Simple and expert interface** (View → Interface or Settings): Simple shows only the basic tools (select, block, entity, texture, decal, clip), short menus and fewer switches; Expert shows everything.

### Interface
**Language:** English or German (Settings → Graphics → Language, applies instantly) – including the descriptions of the built-in entity list (classes, keys, choices, flags, inputs/outputs). Further languages: add a table `assets/lang/<code>.tsv`.
**Dark and light theme** (sun/moon toggle at the top right or Settings → Graphics → Theme):
top bar with menus, command search (Ctrl+K), live session and "Compile & run" with profile selection · tabs with save state and game profile ·
slim tool bar with icons · context bar per tool (shape, undo/redo arrows, grid, snap, snap to vertices, texture lock, visgroups, 1/4 views) ·
viewports with glass labels (switch view/render mode), zoom or FPS, accent-coloured selection with handles, dimensions and label · with a selection, 2D views draw objects on other levels (different depth, ±10 units) fainter · models appear in 2D as their outline instead of a rectangle ·
right panel with Properties / Objects / Visgroups and a texture grid (resizable) · **splitters between the four views can be dragged** (double-click = centre); layout, axes and 3D options are saved, View → "Reset layout" · **map bounds per game profile** (Settings, default ±16384, marked red in 2D) · start page with templates, crash recovery and recently opened maps.
Fonts: Inter and JetBrains Mono (SIL Open Font License, in `assets/fonts`). The 3D view stays dark in both themes.

### Tools (as in Hammer)

| Tool | Key | What it does |
|---|---|---|
| Select | Shift+S | Click, box, Ctrl = add. Handles: scale → rotate (15° steps, Shift = free) → skew |
| Magnify | Shift+G | Click zooms in, right click zooms out |
| Camera | Shift+C | Several 3D cameras, place/move them in 2D, Page Up/Down switches (saved in the VMF) |
| Entity | Shift+E | Place point entities in 2D or directly on surfaces in 3D; class field with suggestions while typing, arrow button opens the full class list |
| Block | Shift+B | Block, wedge, cylinder, spike, sphere, arch, torus (with sides, arc angle, wall thickness, helix height) |
| Face / texture | Shift+A | Select faces, scale/shift/rotation/lightmap/smoothing, fit/left/right/top/bottom/centre, "treat as one", align to world/face, Alt+right click = wrap seamlessly. The right panel then shows only textures (click = apply to selected faces), face settings can be expanded |
| Decal | Shift+D | Place `infodecal` on faces, shown textured in 3D |
| Overlay | Shift+O | Put an `info_overlay` on a face; selected overlays have round corner handles, a rotation handle and a move cross in 3D |
| Clip | Shift+X | Cut brushes along a line (2D) or a plane through **three points in 3D**, preview in 2D and 3D, front/back/both (Shift+X cycles), Enter |
| Vertex | Shift+V | Drag vertices and edges, multi-selection; invalid (non-convex) shapes are rejected |
| Terrain | Shift+P | Sculpt displacements: raise, lower, smooth, sharpen, set height, noise, terraces, paint/erase alpha; soft/linear/hard falloff, Ctrl swaps smooth ↔ sharpen |
| Cordon | Shift+K | Compile only part of the map (sealed automatically with skybox walls) |

### Editing
Undo/redo arrows in the top bar · copy/paste · **Paste special** (copies with offset + rotation) · **carve** · **real hollowing** (also slanted brushes, walls inside or outside; walls never overlap) · **groups** · **hide** (H / Shift+H / U) · **visgroups** + automatic filters (world, detail, triggers, tool textures, lights, props, displacements …) · **transform dialog** (exact move/rotate/scale) · **mirror** · **align objects** · **snap to grid** (the corner of the selection box snaps, as in Hammer) · **replace textures** · **texture lock and scale lock** · **selection mode** groups/objects/single brushes · save & insert **prefabs** · show and collapse **instances** (`func_instance`) – also **nested** (instances inside instances, names and `$` parameters like VBSP), "open instance in tab", saving updates all maps · **grid snapping can be switched off** (Alt+G) · **snap to vertices** of other objects (Alt+V) · **box selection in 3D** · **select touching objects** (Ctrl+Alt+A, Alt while dragging a box) · 2D selection via edges and centre · **rotate towards the mouse with preview** (Ctrl+R) · 3D clicks hit the model's actual triangles, so you can click through holes · undo stores only changes (little memory) · **selection to model** (like Propper, Tools → "Selection to model (prop)…"): brushes are exported as SMD/QC with VertexLitGeneric materials, compiled with studiomdl and replaced by a prop_static/prop_physics

### Displacements
Create (power 2–4), remove, **subdivide** (Catmull-Clark smoothing across several displacements – a cube becomes a sphere), **change power without losing the shape**, noise, sew, invert alpha, **terrain from heightmap** (PNG 8/16 bit or RAW 16 bit → seamless tiles, Map → "Terrain from heightmap…") with a built-in **noise generator** (hill size, detail octaves, roughness, valley floor, terraces, ridges, flat borders, seed; preview, export as 16-bit PNG), collision flags (no phys/hull/ray), brush with radius/strength/direction, alpha display

### Rendering (3D)
Textured (incl. **blend textures** WorldVertexTransition mixed via displacement alpha, `$basetexturetransform`, **animated textures** and **TextureScroll**) · flat · wireframe · lightmap grid · smoothing groups · **light preview with shadows** (light, light_spot, light_environment; the sun and the four nearest lights cast shadows) · **collision models** of props (.phy) as wireframe · **models (MDL)** with textures and skins · editor sprites · light cone helpers · connection lines (outputs) · **3D skybox preview** (sky_camera) · **2D sky** (`skyname` from worldspawn) · **fog** from `env_fog_controller` · **ropes** (`move_rope`/`keyframe_rope` with slack, also in 2D) · pointfile (leak path) · portal file · **frustum culling** for large maps · **anti-aliasing (MSAA)** under Settings → Graphics

### Creating content (Tools menu)
* **Material editor** – create your own materials (VMT), import PNG images as VTF (with mipmaps, normal maps), shader, transparency, reflections, surface property, live preview.
* **Particle editor** – open .pcf files, create systems from templates, edit emitters/initializers/operators/renderers, animated preview, write the particle manifest.
* **Global illumination** – can be switched on in the light preview: indirect light with colour bleeding, computed in the background.
* **Drop with physics** (Object menu) – props fall onto the floor, displacements and each other; Esc cancels.
* **Scatter vegetation** – 3D brush for trees, bushes, rocks (density, spacing, random size/rotation/tilt, Shift erases) or fill a selection.
* **Procedural generators** – maze, rooms with corridors, city block, circular array. Rooms in 3D: own height per room, a corridor network with long straights, round curves (adjustable radius), junctions and loops (each pair of rooms connected at most once), thin walls, short stairs at the room doors (16 units per step), sliders for spacing, winding and extra connections, separate textures for floor, walls and ceiling; every room and corridor is its own group and can be edited separately.
* **Tile set** – place floor, wall, corner, door, window, column, stairs … on a grid; R rotates, Shift removes.
* **Polygon mesh** – free-form meshes from brushes or OBJ: drag points and faces in 3D, extrude, smooth, subdivide; becomes a model via studiomdl as a prop_static (mesh data is stored in `<map>.meshes.json` next to the VMF – copy it along!).
* **Bezier patch** – bend a displacement smoothly with 16 control points.
* Overlays on displacements follow the surface in the 3D view, as in game.
* **Particle preview** – `info_particle_system` plays its effect in the 3D view (View → "Play particle effects").

### Testing and live sync
* **Walk mode** (F6) – walk through the map with player size, gravity and collision: WASD, Space jumps, Ctrl crouches, Shift runs, Esc ends.
* **Test in game from 3D camera** (Shift+F9) – compiles and starts Garry's Mod with the player at the camera; if the game is already running, the map is reloaded there.
* **Live sync** (Compile → "Live sync with the game…") – changes appear instantly in the running game: move and edit entities, new props/entities, deleting, new brushes as a walkable preview, fire inputs, bring the player to the camera or let the camera follow the player. Click "Install bridge" once (creates `addons/amboss_live`) and restart GMod; data is exchanged through `garrysmod/data/amboss_live/`, single player or your own listen server only. The console command `amboss_live_status` shows whether the bridge is running.

### Entities
**456 entity classes built in** (func_, prop_, npc_, trigger_, logic_, env_, info_, point_, filter_, game_, item_, weapon_, ai_, path_, phys_ … with keys, inputs/outputs and descriptions); a real `garrysmod.fgd` is found automatically and takes precedence · **class field with suggestions while typing** and an arrow to the **class picker** (search, favourites, recently used, groups by prefix, keyboard) · **tie to entity** (Ctrl+T) as a dialog · **sound browser with playback** (Windows), **model browser with rotatable 3D preview and textured thumbnails**, **particle browser** (.pcf) · FGD parser (`garrysmod.fgd` incl. `@include`; base classes and entities with the same name, like Valve's `Light`/`light`, are kept apart) · choice lists, colour picker (lights: colour + separate brightness field), spawnflags · **angle compass** · **eyedropper** and name list for targets · **outputs with validation** (red = target missing, orange = unknown input), **suggestions for targets** (names from the map with class, and `!activator`/`!player` …, arrow keys + Enter/Tab) and input selection · incoming connections · **multi-editing** · **copy/paste properties** (Ctrl+Alt+C / V) · help per class · **logical view** (entity I/O as a diagram) · **movement preview** for selected doors/buttons (func_door, func_button, func_movelinear: arrow + end position; func_door_rotating, prop_door_rotating: swing arc + opened door, both directions for doors that open both ways) · model browser directly at the model field (folder button, current model preselected, "Apply" puts it in)
Before compiling, missing keys are filled in with their FGD defaults (as Hammer does when creating an entity) – e.g. a `light` without `_light` would otherwise be black.

### Map
**Check for problems** with auto-fix (Alt+P) · **entity report** · **go to number** (e.g. from VBSP errors) · map info · map properties

### Navmesh (Tools → Navmesh)
Own generator for the NextBot navmesh – without the game and without `nav_generate`: starting at the spawn points, a 25-unit grid is walked (steps up to 18, drops up to 200, player 32×72, otherwise crouched 36, floors up to 45° slope, brushes and displacements; doors count as open). The cells are merged into rectangles and saved as `garrysmod/maps/<map>.nav`. Shown green/blue (crouch) in 2D and 3D; "Load navmesh (.nav)…" also shows existing files. With the compile option "Navmesh" it is created automatically before VBSP and packed into the BSP with "Pack content". Not yet considered: prop collision, ladders, hiding spots/visibility (`nav_analyze`).

### Compile & export
One dialog for everything (Compile → Compile & export…): VVIS/VRAD quality, automatic optimizations, **generate navmesh**, copy the BSP to the maps folder, **pack your own content into the BSP** (expandable, editable file list; "Pack only" packs without recompiling), cubemaps, launch the game. Custom compile steps are available in the expert interface.
**Opening BSP files:** File → Open shows VMF and BSP; a BSP opens the "Decompile BSP" dialog and then the VMF (also via drag & drop onto the window).

### Compiling
Simple mode (VBSP/VVIS/VRAD levels, HDR, extra parameters, cubemaps, copy, launch game) and **expert mode** with freely defined steps · clickable error lines (jump to the brush) · **pointfile is loaded automatically on leaks** · cordon compiling · optional **automatic optimizations** before VBSP · **pack content into BSP** (built in, no bspzip needed; Compile → "Pack content into BSP…"): finds materials incl. bump maps/env maps, models with all files and skins, sounds, particles (.pcf by used systems + materials), soundscapes, navmesh and overview map; editable list, the selection also applies when compiling, addon content stays out by default · **decompile BSP** (File → "Decompile BSP…", like BSPSource): brushes, func_detail, brush entities, displacements, entities, static props, overlays, cubemaps → VMF

### Automatic optimizations (Map → Automatic optimizations…)
Run on demand ("Check only" / "Apply to map" as one undo step) or **automatically on a copy when compiling**
(`amboss_build/<map>.vmf` next to the VMF – your own map stays unchanged, the report is at the top of the compile log). Every optimization can be switched off individually.

| Area | What happens |
|---|---|
| **AI (NPCs)** | `info_node` on all walkable surfaces (adjustable spacing, denser at doors/narrow passages) · **network check**: connections are simulated and shown green, isolated nodes and separated areas marked red/orange · `info_node_air` for flying NPCs · `info_node_link_controller` at doors (OnOpen/OnClose are added) · `info_hint` for cover and narrow passages · navmesh for NextBots |
| **Visibility** | **leak check before VBSP** by flood fill (with red path, compile is aborted) · hidden faces → nodraw · small/slanted brushes not touching the void → `func_detail` (dividing walls and door frames stay world) · `func_areaportal` in door openings · hint brush suggestions (yellow) |
| **Lighting** | lightmap resolution by distance to lights (8/16/32) · distribute `env_cubemap` automatically · light check (light inside a brush, > 32 switchable lights, dark areas) |
| **Geometry** | round vertices to whole units · merge identical boxes · remove slivers and duplicate brushes · simplify flat displacements |
| **Props & entities** | fade distances by model size · disable shadows of small props · remove ineffective logic and duplicate cubemaps · **pack your own content into the BSP** (materials/models lying loose in the game folder) |

Via MCP: `optimize_map` (check or apply, options per run) and `optimize_settings`.

### Merging, comparing, working together (Tools → Versions & merging)
* **Merge map** (File → Merge map…): take over another VMF/.map – keep coordinates, place next to it or move it; duplicate entity names are renamed (references follow), everything goes into a visgroup "Import: …", differing map properties (sky, detail material …) can be chosen individually, warnings for overlaps, two suns, two skybox cameras.
* **Compare with another map version**: new (green), deleted (red) and changed (yellow) objects highlighted in all views, clickable list.
* **3-way merge**: open map + common base version + another person's version. Changes from both sides are combined; real conflicts keep your version, the other one is hidden in the visgroup "Conflict: other version".
* **As a Git merge driver** (two people editing the same VMF in Git):

```bash
git config merge.amboss.driver "amboss --merge3 %O %A %B"
```

  plus in `.gitattributes`: `*.vmf merge=amboss`. Without conflicts the merge runs through automatically.
  `amboss --diff OLD.vmf NEW.vmf` lists the differences.
* **Live collaboration** (Tools → Live session…): one person hosts and shares the open map, others join with address and password.
  Changes appear for everyone immediately; whatever someone has selected is locked for the others and outlined in their colour;
  the other people's cameras are visible in 2D/3D; chat; **Ctrl+Z only undoes your own steps**. Every person gets their own ID range.
  The connection is encrypted (ChaCha20 + HMAC-SHA256, key derived from the password and random values; the password itself is never sent). Default port 3918.
  **Over the internet without a server:** the host listens on IPv4 and IPv6. If both have IPv6, the guest connects directly to the IPv6 address shown in the session window (e.g. `[2001:db8::1]:3918`). The host's router firewall must allow incoming connections on TCP port 3918 to that PC (and possibly the Windows firewall). Without IPv6 it works via IPv4 port forwarding (not with DS-Lite/CGNAT) or a VPN such as Tailscale/ZeroTier. Use a long password for internet sessions.
* Via MCP: `merge_map`, `compare_map`, `merge_versions`.

### New map from template
Start page (can be disabled) and File → New from template…: empty map, room with light and spawn, outdoor area, large building field, arena, two rooms with a door (NPC test) – all sealed and ready to compile.

### Conveniences (borrowed from other editors)
**3D gizmo** (move/rotate) · **Shift+drag duplicates** · **drag and extrude faces in 3D** · **stairs, spiral stairs, ramps, pipes, domes, fences** · **lock layers** and lock objects · **object list (outliner)** · **copy as VMF text** (between tabs and program windows) · **measure tool** · **camera bookmarks** · **go to coordinate** · **command palette** (Ctrl+K) · **freely assignable keyboard shortcuts** · **configurable 3D camera controls** · **recently used and favourite textures** · **texture eyedropper** (Alt+click) · **rotate in fixed steps** · **hidden faces to nodraw automatically** · **.map import** (TrenchBroom/J.A.C.K./Quake) · **OBJ export** (for Blender) · **several maps in tabs** · clickable **undo history** · **Workshop addons (.gma)** as a content source

### Prefab library (File → Prefabs…)
* **14 built-in prefabs** in four categories:
  * *Doors & windows:* sliding door with buttons, rotating door, window with glass
  * *Parts:* ladder, ceiling lamp, stairs with landing, column, physics crate
  * *Tech & logic:* light switch, teleporter pair, damage zone
  * *Rooms:* spawn room, outdoor area with sky, water pool
* **Your own prefabs:** save the selection; subfolders in the prefab folder become categories.
* **Thumbnails** are rendered automatically (tile view with search).
* **Inserting:** drag a tile into a 2D or 3D view, or click it and then click into the view (Shift = several).
  In 3D the prefab aligns to the surface: floor → stands on it, ceiling → hangs from it, wall → sits in front of it.
* **Duplicate entity names** are renamed automatically when inserting (`door` → `door_2`), outputs follow.
* **Hammer libraries (.ol) and .rmf**: shown directly (category "Hammer / …") or converted to VMF files with a button.
* "Copy built-in as VMF" places the library as editable files in the prefab folder.

### MCP – AI remote control
Amboss contains an **MCP server** (Model Context Protocol). An AI assistant such as Claude can use it to build and edit maps:
rooms, shapes, prefabs, entities, outputs, materials, move/rotate/clip, problem check, automatic optimizations, merging and comparing maps, terrain from heightmaps, compiling brushes into models, packing content into the BSP, decompiling BSPs, running Lua scripts, generating navmeshes, saving and compiling (48 tools).

* Runs automatically while Amboss is open: `http://127.0.0.1:3917/mcp` (local only; port in the settings).
* **Claude Code:**

```bash
claude mcp add --transport http amboss http://127.0.0.1:3917/mcp
```

* **Claude Desktop** (`claude_desktop_config.json`):

```json
{ "mcpServers": { "amboss": { "command": "C:\\path\\amboss.exe", "args": ["--mcp-stdio"] } } }
```

  If the editor is running, the AI controls it live; otherwise `--mcp-stdio` works without a window.
* Headless on a file: `amboss --mcp-stdio --headless --map path.vmf [--game-dir …/garrysmod] [--prefab-dir …]`

### Lua scripts (Tools → Lua scripts…)
Built-in **Lua 5.4** for your own tools and repetitive work (stairs, light grids, renaming …). Editor with output, **Ctrl+Enter** runs, scripts live in `skripte/` next to `amboss.exe` (examples are created the first time you open it).
**Every tool of the AI remote control** is available in scripts: `amboss.call("create_brush", {min = {0,0,0}, max = {64,64,64}, material = "DEV/DEV_MEASUREGENERIC01"})`, plus shortcuts: `amboss.brush`, `amboss.room`, `amboss.entity`, `amboss.set`, `amboss.get`, `amboss.output`, `amboss.move`, `amboss.delete`, `amboss.select`, `amboss.selection`, `amboss.entities`, `amboss.info`. `print()` writes to the output. A whole script is **one** undo step; endless loops are stopped after 10 s.

```lua
for i = 0, 11 do  -- stairs
  amboss.brush({i * 16, 0, 0}, {(i + 1) * 16, 64, (i + 1) * 18}, "DEV/DEV_MEASUREGENERIC01")
end
```

### Testing without Garry's Mod
The free Steam title **Source SDK Base 2013 Multiplayer** contains HL2 textures as well as VBSP/VVIS/VRAD.
In *Settings*, set the game folder to `…/Source SDK Base 2013 Multiplayer/hl2` (or `…/hl2mp`) and pick the tools from `…/bin`.
Amboss then reads all `*_dir.vpk` files of the game folder and the base folder `hl2`.

### Content and Workshop
Settings → loading content from addons can be switched off (game folder only) · compiling creates `<map>_addons.txt` listing all required Workshop addons with links (the list is also at the top of the compile log)

### Other
**Game profiles** (several games/installations) · **autosave** · texture browser (filter, "used in map") · VPK/VTF/VMT/MDL read directly from the game content

## Building from source

1. Install Rust: <https://rustup.rs>
2. In the project folder run:

```bash
cargo build --release
```

3. The program is then at `target/release/amboss(.exe)`.

On Linux you also need the usual X11/Wayland libraries (`libxkbcommon-x11-0`, OpenGL).
Valve's compile tools are Windows programs; on Linux enter `wine` as the *wrapper* in the settings.

Windows builds can be cross-compiled from Linux with `cargo build --release --target x86_64-pc-windows-gnu` (needs the MinGW toolchain).

## First steps

1. **Settings → "Find GMod automatically".** This sets the game folder, VBSP/VVIS/VRAD,
   the game executable and `garrysmod.fgd`. The textures are loaded afterwards.
2. **Build a room:** block tool (`Shift+B`), drag a rectangle in the top view and
   then press `Ctrl+H` (hollow).
3. **Textures:** `Shift+T` applies the current texture; `Ctrl+Shift+T` opens the browser. A double click applies the texture to the selection.
4. **Entities:** entity tool (`Shift+E`), e.g. choose `info_player_start` and `light` and
   click in 2D or 3D.
5. **Test:** `F9` saves, compiles and starts GMod with the map.

> Tip: every map must be **sealed**. There must be no hole into the "void", otherwise VBSP reports a *LEAK*.
> A sky is a brush with `TOOLS/TOOLSSKYBOX`. For sunlight use `light_environment`.

## Keyboard shortcuts

| Key | Function |
|---|---|
| `Shift+S G C E B A D O X V P K` | Tools (see above) |
| `Shift+T` / `Ctrl+Shift+T` | Apply current texture / texture browser |
| `[` / `]` | Grid smaller / larger |
| Arrow keys | Move the selection (or vertices) by one grid step |
| `Ctrl+Z` / `Ctrl+Y` | Undo / redo |
| `Ctrl+C / V / X`, `Ctrl+Shift+V` | Copy / paste / cut, paste special |
| `Del` | Delete |
| `Ctrl+G` / `Ctrl+U` | Group / ungroup |
| `H` / `Shift+H` / `U` | Hide selection / hide the rest / show all |
| `Ctrl+T` / `Ctrl+Shift+W` | Tie to entity / move back to world |
| `Ctrl+H` / `Ctrl+Shift+C` | Hollow / carve |
| `Ctrl+M` / `Ctrl+B` | Transform / snap to grid |
| `Ctrl+L` / `Ctrl+I` | Mirror horizontally / vertically (relative to the view under the mouse) |
| `Ctrl+R` | Rotate towards the mouse with preview (around the view axis, 15° steps, Shift = free); left click applies, right click/Esc cancels · `Ctrl+Shift+R` = 90° back |
| `Enter` / `Esc` | Apply clip / cancel or deselect |
| `Alt+P` | Check for problems |
| `Ctrl+Shift+F` / `Ctrl+Shift+G` | Entity report / go to number |
| `Ctrl+E` / `Ctrl+Shift+E` | Centre the 2D / 3D view on the selection |
| `Page Up` / `Page Down` | Switch camera |
| `Shift+Z`, double-click middle mouse button | Maximise view |
| 3D: middle mouse button (wheel pressed) + drag, `W A S D`, `Q`/`E` | Look around, fly, down/up (`Shift` while looking = faster); all configurable under Settings → Keyboard shortcuts → "3D camera" (also right mouse button, sensitivity, invert Y) |
| 2D: mouse wheel, middle mouse button / Space + drag | Zoom, pan |
| `F9` | Compile & run |
| `Shift+F9` | Test in game from 3D camera |
| `F6` | Walk mode (play in editor) |
| `Ctrl+K` | Command palette |
| `Ctrl+F9` | Compile & export dialog |
| `Ctrl+D`, Shift+drag | Duplicate |
| `L` / `Shift+L` | Lock / unlock all |
| `Shift+M` | Measure tool |
| `Shift+W` | 3D gizmo on/off |
| Alt+click (3D) | Texture eyedropper |
| `Ctrl+Shift+1…9` / `Ctrl+1…9` | Save / jump to camera bookmark |
| `Ctrl+Shift+O` / `Ctrl+Shift+H` | Object list / undo history |
| `Ctrl+Tab` / `Ctrl+F4` | Next tab / close tab |
| `Alt+G` / `Alt+V` | Grid snapping on/off / snap to vertices on/off |
| `Ctrl+Alt+A` | Add touching objects to the selection |
| `Ctrl+Alt+C` / `Ctrl+Alt+V` | Copy entity properties / paste onto the selection |
| 3D: drag with left mouse button (select tool) | Box selection (`Ctrl` = add, `Alt` = also touching) |

All shortcuts can be changed under *Settings → Keyboard shortcuts*.

## Code structure

```
src/
  main.rs        Program start (eframe/OpenGL)
  app.rs         Main window: menus, tool bar, shortcuts, views, autosave
  panels.rs      Properties panel, tool options, visgroups, entity editor
  dialogs.rs     Most windows (compile, settings, browsers, problems, report, logical view …)
  editor.rs      Editor state & all operations (GUI-independent, tested)
  map.rs         Data model + reading/writing VMF (groups, visgroups, cameras, cordon, hidden)
  geometry.rs    Planes → polygons, convex hull, AABB, ray tests
  transform.rs   Affine transforms, Source angles
  csg.rs         Plane splitting, carve, hollow, vertex rebuild
  primitives.rs  Basic shapes of the block tool
  disp.rs        Displacements (VMF, geometry, brush)
  problems.rs    Problem check with fixes
  pointfile.rs   .lin / .prt
  view2d.rs      Top/front/side with all tools
  view3d.rs      OpenGL renderer, render modes, light preview, models, skybox
  keyvalues.rs   KeyValues parser/writer
  fgd.rs         FGD parser + built-in GMod entities
  gamefs.rs      VPK reader + game file system
  vtf.rs         VTF decoder
  materials.rs   VMT resolution, texture loading threads
  mdl.rs         MDL/VVD/VTX loader
  models.rs      Model cache (background loading)
  compile.rs     Compile pipeline, expert mode, game profiles, settings
  commands.rs    Command registry, shortcuts, command palette
  mcp.rs         MCP server (HTTP + stdio + headless)
  tabs.rs        Tabs, import/export, MCP binding
  editor2.rs     Duplicating, dragging/extruding faces, locking, nodraw, clipboard, live rotation
  importexport.rs  .map import, OBJ export
  prefabs.rs     Prefab library: built-in prefabs, folders, .ol/.rmf, preview renderer
  prefab_ui.rs   Prefab window, drag & drop, Hammer import
  dialogs2.rs    Command palette, object list, history, shortcut editor, camera controls
  merge.rs       Merging maps (IDs, names, visgroups, warnings)
  merge_ui.rs    Merge / compare / 3-way dialogs
  diff.rs        Comparing map versions, 3-way merge (also for Git)
  collab.rs      Live collaboration: protocol, login, per-object sync, IPv4/IPv6
  netcrypt.rs    Encryption of the live session (ChaCha20, HMAC-SHA256)
  collab_ui.rs   "Live session" window, display of the other participants
  templates.rs   Templates for new maps, start page
  theme.rs       Dark/light colour schemes, fonts, UI building blocks
  icons.rs       Icons (SVG paths from the UI concept) drawn with egui
  chrome.rs      Top bar, tabs, tool bar, context bar, status bar
  optimize.rs    Automatic optimizations: flood fill/leaks, AI nodes, network simulation, func_detail, areaportals …
  opt_ui.rs      Optimization window, integration into the compile flow
  overlay_mesh.rs  Projecting overlays onto faces/displacements
  material_editor.rs, vtf_write.rs   Material editor, PNG → VTF
  dmx.rs, particle_editor.rs         Reading/writing DMX/PCF, particle editor with preview
  gi.rs          Global illumination (BVH, probe grid, background computation)
  physics.rs, place_ui.rs            Physics placement
  scatter.rs     Scattering vegetation
  procgen.rs, procgen_ui.rs, tiles.rs  Generators and tile sets
  mesh.rs, mesh_edit.rs, mesh_ui.rs   Polygon meshes (data, editing, window, studiomdl)
  patch.rs       Bezier patches on displacements
  particle_preview.rs  Particle effects in the 3D view
  model_browser.rs     Model browser with textured thumbnails
  live.rs, live_ui.rs, assets/live/amboss_live.lua   Live sync with the game (Lua bridge)
  walk.rs        Walk mode (player collision as in Source)
```

Tests: `cargo test` (VMF round trip, geometry, FGD, VPK, VTF, materials, compile pipeline, generators, live bridge with a mocked GMod Lua environment …).
The example map `examples/beispiel_raum.vmf` is regenerated with `cargo test write_example -- --ignored`.

Source code comments and most documentation (`CHANGELOG.md`, `ROADMAP.md`, `TESTPLAN_WINDOWS.md`) are in German.

## Translations

The interface exists in German and English (`assets/lang/en.tsv`). Two helper scripts find missing translations:

* `python3 tools/i18n_check.py` – searches the source code for German texts without an entry in the table.
* `AMBOSS_I18N_AUDIT=texts.txt amboss.exe` writes every drawn text into the file; `python3 tools/i18n_audit.py texts.txt` then lists German leftovers in the English interface.

## Not included yet

The complete, checked-off list is in `ROADMAP.md`. The most important open point:

- Editor sprites only appear if the game content contains `materials/editor/*.vmt` (otherwise boxes)

## License

Copyright (c) 2026 Deadshot

Amboss is licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

The bundled fonts are licensed under the SIL Open Font License 1.1:
Inter ([assets/fonts/Inter-OFL.txt](assets/fonts/Inter-OFL.txt)) and
JetBrains Mono ([assets/fonts/JetBrainsMono-OFL.txt](assets/fonts/JetBrainsMono-OFL.txt)).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in Amboss by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.

### Note on Valve

Amboss is an independent project and is not affiliated with Valve Corporation or Facepunch Studios.
"Source", "Hammer", "Half-Life" and "Garry's Mod" are trademarks of their respective owners.
Amboss does not contain any files from Valve; the compile tools (VBSP, VVIS, VRAD) and the
game content come from your own installation.
