# Asset attribution

Every non-code asset in this repository (icons, images, fonts, example art, colour profiles, presets) is listed here with its author, source and licence. `cargo xtask assets` (part of `cargo xtask ci`) fails if an asset file is missing from this table.

**Policy (mandatory):** VectorCraft contains **no Adobe iconography, images, artwork, presets, swatch/brush/symbol/pattern libraries or ICC profiles**. Every asset is either original work by VectorCraft contributors, or third-party material under an open licence (OSI open source, public domain / CC0, or Creative Commons that allows redistribution), and it is attributed below. Screenshots of Adobe software are never committed. Roidery carries none of VectorCraft's ArtCraft trademarks (name, wordmark, logos): VectorCraft's brand licence requires forks to remove them, and they have been removed.

Generated-in-code art is original and has no file to list. This covers the default swatches, brushes, symbols, patterns, graphic styles, image-trace presets and the vector tool cursors (`crates/ui-egui/src/cursors.rs`).

**Font files are never committed here.** Fonts shared by the Crafting Apps live in [storytold/craft-fonts](https://github.com/storytold/craft-fonts) (rules: craftrules [`standards/fonts.md`](https://github.com/storytold/craftrules/blob/main/standards/fonts.md)). The small Latin UI/document fonts below predate that rule and stay.

**Optional build input: craft-fonts.** Builds made with `CRAFT_FONTS_DIR=<craft-fonts checkout>` (all official releases) embed the Japanese fonts listed in its [`ATTRIBUTION.md`](https://github.com/storytold/craft-fonts/blob/main/ATTRIBUTION.md): BIZ UDPGothic Regular and Bold, Shippori Mincho Regular and BIZ UDMincho Regular (web builds: BIZ UDPGothic Regular only), all under the SIL Open Font License 1.1. They are not files in this repository; release packages carry each one's `OFL-<family>.txt`. Builds without `CRAFT_FONTS_DIR` embed none of them.


**Optional install: the embroidery font library.** Roidery's lettering can use digitized embroidery fonts from the Ink/Stitch community ([inkstitch-fonts](https://github.com/inkstitch/inkstitch-fonts)). They are not files in this repository: `cargo run --release -p roidery-fonts --example package_library -- <inkstitch-fonts>/src <out>` packages the ones Roidery may ship (SIL OFL 1.1, CC-BY 4.0, CC-BY-SA 4.0 or public domain; never non-commercial, no-derivatives, GPL or unclear), unchanged apart from compression, each folder with its own `LICENSE`, and writes `FONTS.md` with every font's credits. Installers put the folder beside the program as `embroidery-fonts`. Each font keeps its own licence; none is covered by Roidery's. The fonts packaged from inkstitch-fonts at commit c7e3a05 (2026-08-28):

| Font folder | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `embroidery-fonts/ags_garamond_latin_grec/` | AGS Γαραμου Garamond: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [EBGaramond12](https://github.com/octaviopardo/EBGaramond12) |
| `embroidery-fonts/alchemy/` | Alchemy: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Ouroboros](https://velvetyne.fr/fonts/ouroboros/) |
| `embroidery-fonts/allegria20/` | Allegria 20: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Euphoria Script Regular Extended](https://inkscape.org/cs/~bryce/%E2%98%85euphoria-script-regular-extended) |
| `embroidery-fonts/allegria55/` | Allegria 55: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Euphoria Script Regular Extended](https://inkscape.org/cs/~bryce/%E2%98%85euphoria-script-regular-extended) |
| `embroidery-fonts/ambigue/` | Ambigüe: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Ambidexter](https://fontesk.com/ambidexter-font/) |
| `embroidery-fonts/amitaclo/` | Amitaclo: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Amita](https://fonts.google.com/specimen/Amita) |
| `embroidery-fonts/amitaclo_small/` | Amitaclo small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Amita](https://fonts.google.com/specimen/Amita) |
| `embroidery-fonts/apesplit/` | Apesplit: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Gilda Display](https://fonts.google.com/specimen/Gilda+Display) |
| `embroidery-fonts/apex_lake/` | Apex Lake: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Apex Lake](https://www.1001fonts.com/apex-lake-font.html) |
| `embroidery-fonts/apex_simple_AGS/` | Apex Simple AGS: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Linux Libertine Regular display Bold](https://www.1001freefonts.com/fr/linux-libertine.font) |
| `embroidery-fonts/apex_simple_small_AGS/` | Apex Simple Small AGS: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Linux Libertine Regular display Bold](https://www.1001freefonts.com/fr/linux-libertine.font) |
| `embroidery-fonts/art_nouveau/` | Art Nouveau: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Apollo ASM](https://www.peter-wiegel.de/Apollo.html) |
| `embroidery-fonts/auberge_marif/` | Auberge Marif: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Grand Hotel Marif](https://fonts.google.com/specimen/Grand+Hotel) |
| `embroidery-fonts/auberge_small/` | Auberge Small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Grand Hotel Marif](https://fonts.google.com/specimen/Grand+Hotel) |
| `embroidery-fonts/aventurina/` | Aventurina: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Aventurina](https://dafont.com/fr/aventurina.font) |
| `embroidery-fonts/barstitch_bold/` | Barstitch Bold: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Barlow Bold](https://github.com/jpt/barlow) |
| `embroidery-fonts/barstitch_mandala/` | Barstitch Mandala: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Barlow Bold](https://github.com/jpt/barlow) |
| `embroidery-fonts/barstitch_regular/` | Barstitch regular: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Barlow Regular](https://github.com/jpt/barlow) |
| `embroidery-fonts/bathaus_FI/` | Bathaus FI: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Baumans](https://fonts.google.com/specimen/Baumans) |
| `embroidery-fonts/bathaus_FI_Small/` | Bathaus FI Small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Baumans](https://fonts.google.com/specimen/Baumans) |
| `embroidery-fonts/braille/` | BrailleCC: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font |
| `embroidery-fonts/caesarus_SC_FI/` | Caesarus SC FI: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Marcellus SC](https://fonts.google.com/specimen/Marcellus+SC) |
| `embroidery-fonts/caffeine_KOR/` | Caffeine KOR: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Espresso Dolce](https://www.dafont.com/espresso-dolce.font) |
| `embroidery-fonts/caffeine_tiny/` | Caffeine tiny: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Espresso Dolce](https://www.dafont.com/espresso-dolce.font) |
| `embroidery-fonts/califragilistic/` | Califragilistic: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Ojuju](https://fonts.google.com/specimen/Ojuju) |
| `embroidery-fonts/cats/` | Cats: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Cat Font](https://biostim.com.au/free-fonts/cat-font/) |
| `embroidery-fonts/cherryforinkstitch/` | Cherry for inkstitch: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Cherry AI 1](https://blogfonts.com/cherry-ai-1.font) |
| `embroidery-fonts/cherryforkaalleen/` | Cherry for Kaalleen: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Cherry AI 1](https://blogfonts.com/cherry-ai-1.font) |
| `embroidery-fonts/chicken_little/` | Chicken Little KOR: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Henny Penny](https://fonts.google.com/specimen/Henny+Penny) |
| `embroidery-fonts/chicken_little_small/` | Chicken Little KOR Small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Henny Penny](https://fonts.google.com/specimen/Henny+Penny) |
| `embroidery-fonts/chicken_scratch/` | Chicken Scratch: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Reenie Beanie](https://fonts.google.com/specimen/Reenie+Beanie) |
| `embroidery-fonts/chopin/` | Chopin Script: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Chopin Script](https://www.fonts4free.net/chopin-script-font.html) |
| `embroidery-fonts/cogs_KOR/` | Cogs KOR: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-4.0 | Digitized embroidery font, based on [Jack of Gears](https://michaelwmoss.com/typefaces/jack-of-gears) |
| `embroidery-fonts/cooper_marif/` | CooperMarif: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Cooper*](https://indestructibletype.com/Cooper/index.html) |
| `embroidery-fonts/decadent_flowers_monogram/` | Decadent Flowers Monogram: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Linux Libertine Bold](https://libertine-fonts.org/) |
| `embroidery-fonts/dejavufont/` | Dejavu Serif: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Dejavu Serif Condensed](https://www.fontsquirrel.com/license/dejavu-serif) |
| `embroidery-fonts/digory_doodles_bean/` | Digory Doodles Bean: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Digory Doodles](https://www.fontsquirrel.com/fonts/digory-doodles) |
| `embroidery-fonts/emilio_20/` | Emilio 20: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Emilio 20](https://www.1001fonts.com/emilio-20-font.html) |
| `embroidery-fonts/emilio_20_simple/` | Emilio 20 Simple: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Emilio 20](https://www.1001fonts.com/emilio-20-font.html) |
| `embroidery-fonts/emilio_20_simple_small/` | Emilio 20 Simple Small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Emilio 20](https://www.1001fonts.com/emilio-20-font.html) |
| `embroidery-fonts/emilio_20_tricolore/` | EMILIO 20 TRICOLORE: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Emilio 20](https://www.1001fonts.com/emilio-20-font.html) |
| `embroidery-fonts/emilio_20_applique/` | EMILIO_20_Applique: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Emilio 20](https://www.1001fonts.com/emilio-20-font.html) |
| `embroidery-fonts/emilio_20_bold/` | EMILIO_20_Bold: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Emilio 20](https://www.1001fonts.com/emilio-20-font.html) |
| `embroidery-fonts/emilio_20_tartan/` | EMILIO_20_Tartan: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Emilio 20](https://www.1001fonts.com/emilio-20-font.html) |
| `embroidery-fonts/excalibur_KOR/` | Excalibur KOR: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | Public domain | Digitized embroidery font, based on [Excalibur Nouveau](https://www.dafont.com/excalibur-nouveau.font) |
| `embroidery-fonts/excalibur_small/` | Excalibur small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | Public domain | Digitized embroidery font, based on [Excalibur Nouveau](https://www.dafont.com/excalibur-nouveau.font) |
| `embroidery-fonts/fold_inkstitch/` | Fold Ink/Stitch: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Fold](https://sites.google.com/site/jameskilfiger) |
| `embroidery-fonts/geneva_simple/` | Geneva Simple Sans: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Hershey](http://evilmadscientist.com/go/hershey ) |
| `embroidery-fonts/geneva_rounded/` | Geneva Simple Sans Rounded: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Hershey](http://evilmadscientist.com/go/hershey ) |
| `embroidery-fonts/gingo200/` | gingo200: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font |
| `embroidery-fonts/glacial_tiny/` | Glacial Tiny 60 AGS: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Glacial Indifference](https://www.fontsquirrel.com/fonts/glacial-indifference) |
| `embroidery-fonts/initials_medium/` | Initials Medium: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Sortefax](http://www.glukfonts.pl) |
| `embroidery-fonts/initials_XL/` | Initials XL: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Sortefax](http://www.glukfonts.pl) |
| `embroidery-fonts/inkstitch_masego/` | Ink/Stitch Masego: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Masego](https://madebydebo.gumroad.com/l/hUgEvhttps://madebydebo.gumroad.com/l/hUgEv) |
| `embroidery-fonts/medium_font/` | Ink/Stitch Medium Font: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font |
| `embroidery-fonts/small_font/` | Ink/Stitch Small Font: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font |
| `embroidery-fonts/kum_tsoan_AGS/` | Kum Tsoan AGS: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Namskout](http://www.glukfonts.pl/font.php?font=Namskout) |
| `embroidery-fonts/learning_curve/` | Learning curve: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Learning Curve](https://www.bvfonts.com/fonts/details.php?id=100) |
| `embroidery-fonts/magnolia_bicolor/` | Magnolia bicolor: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Magnolia Script](https://www.dafont.com/magnolia-script.font) |
| `embroidery-fonts/magnolia_KOR/` | Magnolia KOR: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Magnolia Script](https://www.dafont.com/magnolia-script.font) |
| `embroidery-fonts/magnolia_small/` | Magnolia Small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Magnolia Script](https://www.dafont.com/magnolia-script.font) |
| `embroidery-fonts/magnolia_tamed/` | Magnolia tamed: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Magnolia Script](https://www.dafont.com/magnolia-script.font) |
| `embroidery-fonts/mai_en_fleur/` | Mai En Fleur AGS: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Abril Fatface](https://fonts.google.com/specimen/Abril+Fatface) |
| `embroidery-fonts/mam_script/` | MAM Script: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Kaushan Script](https://fonts.google.com/specimen/Kaushan+Script) |
| `embroidery-fonts/manga_impact/` | Manga Impact: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Shojumaru](https://fonts.google.com/specimen/Shojumaru) |
| `embroidery-fonts/marifenda/` | Marifenda: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Merienda-Bold](https://github.com/etunni/merienda) |
| `embroidery-fonts/milli_marif_bold/` | Millimarif-bold20: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Millimetre](https://gitlab.com/StudioTriple/Millimetre) |
| `embroidery-fonts/mimosa_large/` | Mimosa Large: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Nose Transport](https://fontesk.com/nose-transport-font/) |
| `embroidery-fonts/mimosa_medium/` | Mimosa Medium: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Nose Transport](https://fontesk.com/nose-transport-font/) |
| `embroidery-fonts/monicha/` | Monicha: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | CC-BY-SA-4.0 | Digitized embroidery font, based on [Monicha](https://dafont.com) |
| `embroidery-fonts/montecarlo/` | Montecarlo: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Montecarlo Regular](https://fonts.google.com/specimen/MonteCarlo) |
| `embroidery-fonts/neon/` | Néon: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Sportrop](https://www.glukfonts.pl/font.php?font=Sportrop) |
| `embroidery-fonts/neon_blinking/` | Néon blinking: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Sportrop](https://www.glukfonts.pl/font.php?font=Sportrop) |
| `embroidery-fonts/pacificlo/` | Pacificlo: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Pacifico](https://fonts.google.com/specimen/Pacifico) |
| `embroidery-fonts/pacificlo_tiny/` | Pacificlo tiny: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Pacifico](https://fonts.google.com/specimen/Pacifico) |
| `embroidery-fonts/paquerette/` | Paquerette: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | Public domain | Digitized embroidery font, based on [Coronaviral](https://fontesk.com/coronaviral-font/) |
| `embroidery-fonts/pisankris/` | Pisankris: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Manuskript Gothisch](http://www.peter-wiegel.de/Fonts/Pisankris.htm) |
| `embroidery-fonts/pixel10/` | Pixel 10: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Jersey 10 Regular](https://github.com/scfried/soft-type-jersey) |
| `embroidery-fonts/roaring_twenties_KOR/` | Roaring Twenties KOR: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Limelight](https://fonts.google.com/specimen/Limelight) |
| `embroidery-fonts/roaring_twenties_KOR_small/` | Roaring Twenties KOR Small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Limelight](https://fonts.google.com/specimen/Limelight) |
| `embroidery-fonts/roman_ags/` | Roman AGS: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Latin Modern Roman 10 Bold Italic](https://www.gust.org.pl/projects/e-foundry/latin-modern) |
| `embroidery-fonts/roman_ags_bicolor/` | Roman bicolor AGS: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Latin Modern Roman 10 Bold Italic](https://www.gust.org.pl/projects/e-foundry/latin-modern) |
| `embroidery-fonts/sacramarif/` | Sacramarif: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Sacramento](https://fonts.google.com/specimen/Sacramento) |
| `embroidery-fonts/stebor_AGS/` | Stebor AGS: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Lobster Two Bold Italic](https://fonts.google.com/specimen/Lobster+Two) |
| `embroidery-fonts/sunset/` | Sunset: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Big Shoulders Inline](https://fontesk.com/big-shoulders-inline-font/ ) |
| `embroidery-fonts/tt_directors/` | TT Directors: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [TT Directors](https://www.myfonts.com/fonts/tt-arts/tt-directors/) |
| `embroidery-fonts/tt_masters/` | TT Masters: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [TT Masters](www.free-fonts-download.com/Script/tt-masters-font) |
| `embroidery-fonts/venezia/` | Venezia: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Andada Pro](https://fonts.google.com/specimen/Andada+Pro) |
| `embroidery-fonts/venezia_small/` | Venezia small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Andada Pro](https://fonts.google.com/specimen/Andada+Pro) |
| `embroidery-fonts/violin_serif/` | Violin Serif: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Instrument Serif]( https://fonts.google.com/specimen/Instrument+Serif) |
| `embroidery-fonts/western_light/` | Western Light: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Sancreek](https://fonts.google.com/specimen/Sancreek) |
| `embroidery-fonts/ellenika/` | Ελληνικά — Greek satin font: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Clara Bold](https://fontesk.com/clara-typeface/) |
| `embroidery-fonts/cyrillic/` | Кирилиця: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Roboto](https://fonts.google.com/specimen/Roboto) |
| `embroidery-fonts/hebrew_font_large/` | חוכמה Large: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Schlomo](https://github.com/aharonium/fonts/tree/master/Fonts/Hebrew%20Letters%20with%20Vowels%20and%20Cantillation/Shlomo%20Orbach%20(OFL)) |
| `embroidery-fonts/hebrew_font_medium/` | חוכמה Medium: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Schlomo](https://github.com/aharonium/fonts/tree/master/Fonts/Hebrew%20Letters%20with%20Vowels%20and%20Cantillation/Shlomo%20Orbach%20(OFL)) |
| `embroidery-fonts/hebrew_simple_rounded/` | פשוט מעוגל: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Varela Round Hebrew](https://github.com/alefalefalef/Varela-Round-Hebrew) |
| `embroidery-fonts/hebrew_simple_rounded_small/` | פשוט מעוגל Small: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Varela Round Hebrew](https://github.com/alefalefalef/Varela-Round-Hebrew) |
| `embroidery-fonts/honoka/` | コルの書きこも: Ink/Stitch community (see its `LICENSE`) | https://github.com/inkstitch/inkstitch-fonts | OFL-1.1 | Digitized embroidery font, based on [Honoka Antique Kaku](https://font.gloomy.jp/honoka-antique-kaku-dl.html) |

| Asset | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `assets/app-icon/LICENSE.txt` | (licence/readme text) | VectorCraft | MIT OR Apache-2.0 |  |
| `assets/app-icon/README.md` | (licence/readme text) | VectorCraft | MIT OR Apache-2.0 |  |
| `assets/app-icon/hicolor/128x128/apps/ai.storyteller.vectorcraft.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/hicolor/16x16/apps/ai.storyteller.vectorcraft.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/hicolor/24x24/apps/ai.storyteller.vectorcraft.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/hicolor/256x256/apps/ai.storyteller.vectorcraft.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/hicolor/32x32/apps/ai.storyteller.vectorcraft.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/hicolor/48x48/apps/ai.storyteller.vectorcraft.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/hicolor/512x512/apps/ai.storyteller.vectorcraft.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/hicolor/64x64/apps/ai.storyteller.vectorcraft.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/hicolor/scalable/apps/ai.storyteller.vectorcraft.svg` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/vectorcraft-1024.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/vectorcraft-macos-512.png` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/vectorcraft-small.svg` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/vectorcraft.icns` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/vectorcraft.ico` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/app-icon/vectorcraft.svg` | Project owner (ArtCraft team) | Original artwork drawn in ArtCraft for VectorCraft, vectorised and rendered by `packaging/icons.sh` | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon (engraved dragon) |
| `assets/roidery-icon/LICENSE.txt` | (licence/readme text) | Roidery | MIT OR Apache-2.0 |  |
| `assets/roidery-icon/README.md` | (licence/readme text) | Roidery | MIT OR Apache-2.0 |  |
| `assets/roidery-icon/apple-touch-icon.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/favicon.ico` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/hicolor/128x128/apps/app.roidery.Roidery.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/hicolor/16x16/apps/app.roidery.Roidery.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/hicolor/24x24/apps/app.roidery.Roidery.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/hicolor/256x256/apps/app.roidery.Roidery.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/hicolor/32x32/apps/app.roidery.Roidery.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/hicolor/48x48/apps/app.roidery.Roidery.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/hicolor/512x512/apps/app.roidery.Roidery.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/hicolor/64x64/apps/app.roidery.Roidery.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/hicolor/scalable/apps/app.roidery.Roidery.svg` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/roidery-1024.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/roidery-macos-512.png` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/roidery.icns` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/roidery.ico` | Roidery contributors | Rendered from `assets/roidery-icon/roidery.svg` (`packaging/roidery-icons.sh`) | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon |
| `assets/roidery-icon/roidery.svg` | Roidery contributors | Original work, drawn by hand for Roidery | MIT OR Apache-2.0 (`assets/roidery-icon/LICENSE.txt`) | App icon (an R sewn in satin in a hoop) |
| `packaging/macos/dmg/background.svg` | Xus Badia (contributed in #493) | Original artwork for VectorCraft's DMG window; links the app icon `assets/app-icon/vectorcraft-small.svg` | MIT OR Apache-2.0 (licensed by the contributor under the project licence) | DMG background source. Text in Inter and JetBrains Mono (OFL-1.1, `assets/fonts`), not embedded; paper grain generated by an SVG `feTurbulence` filter |
| `packaging/macos/dmg/background.tiff` | Xus Badia (contributed in #493) | Rendered from `packaging/macos/dmg/background.svg` by `packaging/macos/dmg/generate.py` | MIT OR Apache-2.0 | 1x + 2x DMG background; includes the app icon (see its rows), text rendered from OFL-1.1 fonts and LittleCMS's built-in sRGB profile ("No copyright, use freely") |
| `packaging/macos/dmg/DS_Store` | Xus Badia (layout, #493) | Written by `packaging/macos/dmg/generate.py` | MIT OR Apache-2.0 | Finder view settings for the DMG (window, icon size and positions, background alias); no machine data |
| `assets/fonts/Inter-Medium.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface, not product iconography or artwork |
| `assets/fonts/Inter-Regular.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface, not product iconography or artwork |
| `assets/fonts/Inter-SemiBold.ttf` | Rasmus Andersson / The Inter Project Authors | https://github.com/rsms/inter | OFL-1.1 (`assets/fonts/OFL-Inter.txt`) | Open-source typeface, not product iconography or artwork |
| `assets/fonts/JetBrainsMono-Regular.ttf` | The JetBrains Mono Project Authors | https://github.com/JetBrains/JetBrainsMono | OFL-1.1 (`assets/fonts/OFL-JetBrainsMono.txt`) | Open-source typeface, not product iconography or artwork |
| `assets/fonts/SourceSans3-Bold.ttf` | Paul D. Hunt / Adobe (released as open source) | https://github.com/adobe-fonts/source-sans | OFL-1.1 (`assets/fonts/OFL-SourceSans3.txt`) | Open-source typeface, not product iconography or artwork |
| `assets/fonts/SourceSans3-It.ttf` | Paul D. Hunt / Adobe (released as open source) | https://github.com/adobe-fonts/source-sans | OFL-1.1 (`assets/fonts/OFL-SourceSans3.txt`) | Open-source typeface, not product iconography or artwork |
| `assets/fonts/SourceSans3-Regular.ttf` | Paul D. Hunt / Adobe (released as open source) | https://github.com/adobe-fonts/source-sans | OFL-1.1 (`assets/fonts/OFL-SourceSans3.txt`) | Open-source typeface, not product iconography or artwork |
| `assets/fonts/SourceSans3-Semibold.ttf` | Paul D. Hunt / Adobe (released as open source) | https://github.com/adobe-fonts/source-sans | OFL-1.1 (`assets/fonts/OFL-SourceSans3.txt`) | Open-source typeface, not product iconography or artwork |
| `assets/fonts/SourceSerif4-Regular.ttf` | Frank Grießhammer / Adobe (released as open source) | https://github.com/adobe-fonts/source-serif | OFL-1.1 (`assets/fonts/OFL-SourceSerif4.txt`) | Open-source typeface, not product iconography or artwork |
| `assets/fonts/OFL-Inter.txt` | (licence text) | upstream project | — |  |
| `assets/fonts/OFL-JetBrainsMono.txt` | (licence text) | upstream project | — |  |
| `assets/fonts/OFL-SourceSans3.txt` | (licence text) | upstream project | — |  |
| `assets/fonts/OFL-SourceSerif4.txt` | (licence text) | upstream project | — |  |
| `assets/icons/LICENSE-lucide.txt` | (licence text) | upstream project | — |  |
| `assets/icons/align-center-horizontal.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-center-vertical.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-end-horizontal.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-end-vertical.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-horizontal-justify-center.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-horizontal-justify-end.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-horizontal-justify-start.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-start-horizontal.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-start-vertical.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-vertical-justify-center.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-vertical-justify-end.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/align-vertical-justify-start.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/arrow-left-right.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/arrow-up-down.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/blend.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/brush.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/chart-column.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/chevron-down.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/chevron-left.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/chevron-right.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/chevrons-left.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/chevrons-right.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/circle.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/clipboard-paste.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/cloud.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/combine.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/copy.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/dc-actions.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-al-bottom.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-al-center.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-al-hcenter.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-al-left.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-al-right.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-al-top.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-al-vcenter.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-align.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-alignto-artboard.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-alignto-key.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-alignto-selection.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-anchor.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-anchor-smooth.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-appearance.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-arc.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-arrow-down.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-arrow-extend.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-arrow-fill.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 | The Selection and Direct Selection arrows' fill, painted black or white under their outline |
| `assets/icons/dc-arrow-tip.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-arrow-up.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-artboard-options.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-artboards.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-blend.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-bloat.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-cap-butt.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-cap-round.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-cap-square.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-center-hide.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-center-show.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-clear.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-color-guide.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-crystallize.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-cube.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dash-align.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dash-exact.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dir-off.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dir-on.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-direct.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dist-bottom.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dist-hcenter.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dist-hspace.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dist-left.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dist-right.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dist-top.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dist-vcenter.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-dist-vspace.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-draw-behind.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-draw-inside.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-draw-normal.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-ellipse.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-fill-none.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-flare.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-folder.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-free-transform.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-fx.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-gamut.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-grad-freeform.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-grad-linear.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-grad-radial.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-grad-stroke-across.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-grad-stroke-along.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-grad-stroke-within.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-gradient.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-graphic-styles.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-grid-view.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-group-select.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-join-bevel.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-join-miter.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-join-round.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-join.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-knife.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-line-cut.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-line.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-list-view.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-live-bucket.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-live-select.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-mask-none.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-measure.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-mesh.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-mirror-cut.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-new-fill.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-new-item.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-new-stroke.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-options.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-para-center.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-para-justify-all.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-para-justify-center.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-para-justify-left.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-para-justify-right.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-para-left.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-para-ltr.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-para-right.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-para-rtl.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-path-eraser.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pathfinder.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-perspective.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-crop.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-divide.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-exclude.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-intersect.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-merge.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-minus-back.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-minus-front.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-outline.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-trim.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pf-unite.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-place-symbol.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-polar-grid.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-polygon.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-pucker.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-puppet.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-rearrange.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-rect-cut.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-rect-grid.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-reference-point.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-remove-brush.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-reshape.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-reverse.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-rotate-view.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-rounded-rect.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-rule-evenodd.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-rule-nonzero.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-scallop.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-screen-mode.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-selection.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-shape-builder.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-shear.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-smooth.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-stroke-center.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-stroke-inside.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-stroke-outside.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-stroke.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-swap.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-swatch-kinds.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-symbol-sprayer.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-touch-type.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-transform-panel.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-transparency.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-twirl.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-type-area.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-type-path.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-type-vertical.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-width-profile.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-width.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-wrinkle.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-zoom-large.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/dc-zoom-small.svg` | VectorCraft contributors | Original work, drawn for VectorCraft | MIT OR Apache-2.0 |  |
| `assets/icons/rd-satin-blocks.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Satin Blocks tool |
| `assets/icons/rd-density.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Embroidery bar: Density Map |
| `assets/icons/rd-guides.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Embroidery bar: Guides |
| `assets/icons/rd-hoop.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Embroidery bar: Hoop |
| `assets/icons/rd-jumps.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Embroidery bar: Jumps and Trims |
| `assets/icons/rd-needle-points.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Embroidery bar: Needle Points |
| `assets/icons/rd-snap.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Embroidery bar: Snap |
| `assets/icons/rd-solo-color.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Embroidery bar: Current Color Only |
| `assets/icons/rd-underlay.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Embroidery bar: Underlay |
| `assets/icons/rd-fill-angle.svg` | Roidery contributors | Original work, drawn for Roidery | MIT OR Apache-2.0 | Fill Angle tool |
| `assets/icons/ellipsis.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/eraser.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/eye-off.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/eye.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/file-plus.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/flip-horizontal-2.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/flip-vertical-2.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/folder-open.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/frame.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/grid-3x3.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/group.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/hand.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/hexagon.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/history.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/house.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/image.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/info.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/lasso.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/layers.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/layout-grid.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/library.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/link-2-off.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/link.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/external-link.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Help → app page link |
| `assets/icons/git-branch.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) | Help → GitHub link |
| `assets/icons/globe.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) | Help → website link |
| `assets/icons/message-circle.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) | Discord button and Help → Discord link |
| `assets/icons/lock-open.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/lock.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/map.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/maximize-2.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/menu.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/minus.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/monitor.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/move-diagonal-2.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/move.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/paint-bucket.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/paintbrush.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/palette.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/pause.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: sew-out player |
| `assets/icons/pen-tool.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/pen-tool-add.svg` | Lucide Icons and Contributors; badge by VectorCraft contributors | https://lucide.dev (lucide-static v1.49.0 `pen-tool`; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) | Lucide's pen-tool with a plus badge |
| `assets/icons/pen-tool-delete.svg` | Lucide Icons and Contributors; badge by VectorCraft contributors | https://lucide.dev (lucide-static v1.49.0 `pen-tool`; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) | Lucide's pen-tool with a minus badge |
| `assets/icons/pencil.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/pilcrow.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/pin.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/pipette.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/play.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: sew-out player |
| `assets/icons/plus.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/printer.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/redo-2.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/rotate-3d.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/rotate-ccw.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/rotate-cw.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/ruler.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/save.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/scaling.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/scissors.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/search.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/settings.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/shapes.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/share-2.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/skip-back.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: sew-out player |
| `assets/icons/skip-forward.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: sew-out player |
| `assets/icons/slice.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/sparkles.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/spline-pointer.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/spline.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/spool.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: Stitch Settings panel |
| `assets/icons/list-ordered.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: Sequencer tab |
| `assets/icons/arrow-down-wide-narrow.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: Sequencer Sort by Color |
| `assets/icons/chevron-up.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: Sequencer Sew Earlier |
| `assets/icons/rainbow.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: Threads panel |
| `assets/icons/import.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) | Roidery: Threads panel Import Chart |
| `assets/icons/spray-can.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/square-dashed.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/square.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/squares-exclude.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/squares-intersect.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/squares-subtract.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/squares-unite.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/squircle.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/star.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/sun.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/swatch-book.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/text-cursor-input.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/tornado.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/trash-2.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/triangle.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/triangle-alert.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/type.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/undo-2.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/ungroup.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/wand-sparkles.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/waves.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/x.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `assets/icons/zoom-in.svg` | Lucide Icons and Contributors | https://lucide.dev (lucide-static v1.49.0; header kept in file) | ISC; derived from Feather, also MIT (`assets/icons/LICENSE-lucide.txt`) |  |
| `docs/images/roidery-flower.png` | Roidery contributors | Screenshot of Roidery itself (a flower digitized in Roidery, realistic view) | MIT OR Apache-2.0 |  |
| `docs/images/dusk-poster.png` | VectorCraft contributors | Rendered by VectorCraft from `examples/dusk-poster.vectorcraft` | MIT OR Apache-2.0 |  |
| `examples/dusk-poster.vectorcraft` | VectorCraft contributors | Original artwork built through the VectorCraft command API | MIT OR Apache-2.0 |  |
| `examples/dusk-poster.svg` | VectorCraft contributors | SVG export of the above | MIT OR Apache-2.0 |  |
| `docs/images/shot-1-neon.png` | VectorCraft contributors | Screenshot of VectorCraft itself (VectorCraft/Lucide UI icons only), editing `examples/neon-drive.vectorcraft` | MIT OR Apache-2.0 |  |
| `docs/images/shot-2-ribbons.png` | VectorCraft contributors | Screenshot of VectorCraft itself, editing `examples/ribbons.vectorcraft` | MIT OR Apache-2.0 |  |
| `docs/images/shot-3-sheet.png` | VectorCraft contributors | Screenshot of VectorCraft itself, editing `examples/feature-sheet.vectorcraft` | MIT OR Apache-2.0 |  |
| `docs/images/shot-4-bezier.png` | VectorCraft contributors | Screenshot of VectorCraft itself, editing `examples/feature-sheet.vectorcraft` | MIT OR Apache-2.0 |  |
| `docs/images/shot-5-perspective.png` | VectorCraft contributors | Screenshot of VectorCraft itself, editing `examples/perspective-city.vectorcraft` | MIT OR Apache-2.0 |  |
| `docs/images/art-neon-drive.png` | VectorCraft contributors | Rendered by VectorCraft from `examples/neon-drive.vectorcraft` | MIT OR Apache-2.0 |  |
| `docs/images/art-ribbons.png` | VectorCraft contributors | Rendered by VectorCraft from `examples/ribbons.vectorcraft` | MIT OR Apache-2.0 |  |
| `docs/images/art-pathfinder.png` | VectorCraft contributors | Rendered by VectorCraft from `examples/feature-sheet.vectorcraft` (artboard 1) | MIT OR Apache-2.0 |  |
| `docs/images/art-mesh.png` | VectorCraft contributors | Rendered by VectorCraft from `examples/feature-sheet.vectorcraft` (artboard 2) | MIT OR Apache-2.0 |  |
| `docs/images/art-repeat.png` | VectorCraft contributors | Rendered by VectorCraft from `examples/feature-sheet.vectorcraft` (artboard 3) | MIT OR Apache-2.0 |  |
| `docs/images/art-envelope.png` | VectorCraft contributors | Rendered by VectorCraft from `examples/feature-sheet.vectorcraft` (artboard 4) | MIT OR Apache-2.0 |  |
| `examples/neon-drive.vectorcraft` | VectorCraft contributors | Original artwork built through the VectorCraft command API | MIT OR Apache-2.0 |  |
| `examples/ribbons.vectorcraft` | VectorCraft contributors | Original artwork built through the VectorCraft command API | MIT OR Apache-2.0 |  |
| `examples/feature-sheet.vectorcraft` | VectorCraft contributors | Original artwork built through the VectorCraft command API | MIT OR Apache-2.0 |  |
| `examples/perspective-city.vectorcraft` | VectorCraft contributors | Original artwork built through the VectorCraft command API | MIT OR Apache-2.0 |  |
