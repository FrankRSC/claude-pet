//! Avatares extra. Cada uno es un sprite de 16×16 con su paleta; las filas de
//! 8 caracteres se reflejan (sprite simétrico) y las de 16 se usan tal cual.
//! Las poses y el contorno se generan en `sprites::build`.

use crate::sprites::Color;

pub struct Skin {
    pub name: &'static str,
    pub rows: [&'static str; 16],
    pub palette: &'static [(u8, Color)],
    /// Carácter de los ojos, para poder cerrarlos al parpadear o dormir.
    pub eye: u8,
}

const K: (u8, Color) = (b'k', [34, 24, 30]);
const W: (u8, Color) = (b'w', [245, 245, 245]);
const SKIN: (u8, Color) = (b's', [250, 210, 170]);

const FLAME: [&str; 16] = [
    "........",
    "....r...",
    "...rr..r",
    "..rrrrrr",
    "..rroorr",
    ".rrooooo",
    ".rooyyyy",
    ".rooyyyy",
    ".roykyyy",
    ".roykyyy",
    ".rooyyyy",
    ".rooyyyk",
    "..rooyyy",
    "..rroooo",
    "...rrooo",
    ".....rrr",
];

pub const SKINS: &[Skin] = &[
    Skin {
        name: "Gato",
        rows: [
            "........", "........", "..g.....", "..gg....", "..gpgggg", "..gggggg", "..ggkggg", "..ggkggg",
            "..wggggp", "...ggggg", "...gwwww", "...gwwww", "...gwwww", "...ggggg", "...gg...", "...ww...",
        ],
        palette: &[(b'g', [150, 150, 165]), (b'p', [240, 150, 170]), W, K],
        eye: b'k',
    },
    Skin {
        name: "Perro",
        rows: [
            "........", "........", "..o.....", "..oo....", "..owoooo", "..oooooo", "..ookooo", "..ookoow",
            "..oowwww", "...owwwk", "...owwww", "...owwww", "...ooooo", "...ooooo", "...oo...", "...ww...",
        ],
        palette: &[(b'o', [220, 135, 60]), W, K],
        eye: b'k',
    },
    Skin {
        name: "Robot",
        rows: [
            "........", ".......g", ".......b", "...wwwww", "..wwwwww", "..wddddd", "..wddbdd", "..wddbdd",
            "..wwwwww", "...wwwww", "..gwwwbb", "..gwwwww", "...wwwww", "...ggggg", "...ww...", "...gg...",
        ],
        palette: &[(b'w', [230, 236, 248]), (b'g', [150, 160, 180]), (b'd', [30, 40, 70]), (b'b', [90, 190, 255])],
        eye: b'b',
    },
    Skin {
        name: "Fantasma",
        rows: [
            "........", "........", "........", ".....www", "....wwww", "...wwwww", "...wwkww", "...wwkww",
            "..wwwwww", "..wwwwww", "..wwwwww", "..wwwwww", "..wwwwww", "..wwwwww", "..ssssss", "..ss.ss.",
        ],
        palette: &[(b'w', [245, 245, 252]), (b's', [200, 205, 228]), K],
        eye: b'k',
    },
    Skin {
        name: "Alien",
        rows: [
            "........", "........", "...ggggg", "..gggggg", ".ggggggg", ".ggkkggg", ".gggkkgg", "..gggggg",
            "...ggggg", "....gggg", "...ggggg", "..dggggg", "..dggggg", "...ggggg", "...gg...", "...gg...",
        ],
        palette: &[(b'g', [120, 215, 85]), (b'd', [65, 150, 55]), K],
        eye: b'k',
    },
    Skin {
        name: "Panda",
        rows: [
            "........", "........", "..bb....", "..bbwwww", "..wwwwww", "..wbbbww", "..wbebww", "..wbbbww",
            "..wwwwwb", "...wwwww", "..bbwwww", "..bbwwww", "...wwwww", "...wwwww", "...bb...", "...bb...",
        ],
        palette: &[W, (b'b', [40, 40, 48]), (b'e', [250, 250, 250])],
        eye: b'e',
    },
    Skin {
        name: "Pingüino",
        rows: [
            "........", "........", "....bbbb", "...bbbbb", "..bbbbbb", "..bbwwww", "..bwkwww", "..bwkwwo",
            "..bwwwww", "..bbwwww", ".bbwwwww", ".bbwwwww", "..bwwwww", "..bbwwww", "...oo...", "..ooo...",
        ],
        palette: &[(b'b', [45, 52, 78]), W, (b'o', [245, 160, 40]), K],
        eye: b'k',
    },
    Skin {
        name: "Zorro",
        rows: [
            "........", "........", "..o.....", "..oo....", "..owoooo", "..oooooo", "..ookooo", "..ookooo",
            "..owwwww", "...wwwwk", "...owwww", "...owwww", "...ooooo", "...ooooo", "...dd...", "...dd...",
        ],
        palette: &[(b'o', [238, 112, 40]), (b'd', [80, 45, 35]), W, K],
        eye: b'k',
    },
    Skin {
        name: "Conejo",
        rows: [
            "...ww...", "...wp...", "...wp...", "...ww...", "..wwwwww", "..wwwwww", "..wwkwww", "..wwkwww",
            "..pwwwwp", "...wwwww", "...wwwww", "..wwwwww", "..wwwwww", "...wwwww", "...ww...", "...ww...",
        ],
        palette: &[(b'w', [248, 244, 248]), (b'p', [245, 160, 185]), K],
        eye: b'k',
    },
    Skin {
        name: "Dinosaurio",
        rows: [
            "................",
            "................",
            "........ggggggg.",
            ".......ggggggggg",
            ".......ggkgggggg",
            ".......ggkgggggg",
            "......dggggggggg",
            ".....dggggggg...",
            "....dggggggg....",
            "...dgggyyyg.gg..",
            "..dggggyyyg.....",
            ".gggggggyyg.....",
            "gg.gggggggg.....",
            "....gggggg......",
            "....gg..gg......",
            "....gg..gg......",
        ],
        palette: &[(b'g', [85, 195, 75]), (b'd', [40, 125, 45]), (b'y', [240, 222, 120]), K],
        eye: b'k',
    },
    Skin {
        name: "Nube",
        rows: [
            "........", "........", "........", "........", "........", ".....www", "...wwwww", "..wwwwww",
            ".wwwwwww", ".wwwkwww", "wwwwkwww", "wwpwwwwk", "wwwwwwww", "wwwwwwww", ".ccccccc", "..cccc..",
        ],
        palette: &[(b'w', [248, 250, 255]), (b'c', [185, 212, 245]), (b'p', [250, 175, 195]), K],
        eye: b'k',
    },
    Skin {
        name: "Esqueleto",
        rows: [
            "........", "........", "...wwwww", "..wwwwww", "..wwwwww", "..wkkwww", "..wkkwww", "..wwwwwk",
            "...wwwww", "...wkwkw", "......ww", "..w.wwww", "..w.wkwk", "....wwww", "....w...", "...ww...",
        ],
        palette: &[(b'w', [236, 236, 224]), K],
        eye: b'k',
    },
    Skin {
        name: "Bruja",
        rows: [
            ".......p", "......pp", ".....ppp", "....pppp", "...yyyyy", ".ppppppp", "..hsssss", "..hsksss",
            "..hsksss", "...sssss", "..rrrrrr", ".srrrrrr", "..rrrrrr", "..rrrrrr", "...kk...", "...kk...",
        ],
        palette: &[
            (b'p', [110, 60, 170]),
            (b'y', [245, 205, 60]),
            (b'h', [70, 45, 90]),
            (b'r', [140, 85, 210]),
            SKIN,
            K,
        ],
        eye: b'k',
    },
    Skin {
        name: "Fuego",
        rows: FLAME,
        palette: &[(b'r', [230, 60, 30]), (b'o', [250, 145, 30]), (b'y', [255, 222, 90]), K],
        eye: b'k',
    },
    Skin {
        name: "Planta",
        rows: [
            "..gg....", ".gggg...", "..gggg.d", ".......d", "...sssss", "..ssssss", "..ssksss", "..ssksss",
            "..psssss", "...sssss", "...sssss", "..ssssss", "..ssssss", "...sssss", "...bb...", "...bb...",
        ],
        palette: &[
            (b'g', [95, 195, 75]),
            (b'd', [55, 135, 55]),
            (b's', [242, 218, 165]),
            (b'b', [180, 130, 80]),
            (b'p', [245, 165, 170]),
            K,
        ],
        eye: b'k',
    },
    Skin {
        name: "Hielo",
        rows: [
            ".......c", "......cc", ".....ccc", "....wccc", "...wcccc", "..wccccc", "..cckccc", "..cckccc",
            "..cccccc", "..bccccc", "...bcccc", "...bbccc", "....bbcc", "....bbbb", "....bb..", "....bb..",
        ],
        palette: &[(b'c', [165, 222, 255]), (b'b', [80, 160, 240]), (b'w', [240, 250, 255]), K],
        eye: b'k',
    },
    Skin {
        name: "Roca",
        rows: [
            "........", "........", "...ggggg", "..glgggg", "..gggggg", "..gggggg", "..ggkggg", "..ggkggg",
            "..gggddd", "..dggggg", ".ggggggg", ".glggggg", ".ggggggg", "..dggggg", "..ddd...", "..ddd...",
        ],
        palette: &[(b'g', [145, 145, 150]), (b'd', [95, 95, 102]), (b'l', [190, 190, 195]), K],
        eye: b'k',
    },
    Skin {
        name: "Ninja",
        rows: [
            "........", "........", "...bbbbb", "..bbbbbb", ".rrrrrrr", "..bbbbbb", "..bsesss", "..bsesss",
            "..bbbbbb", "...bbbbb", "..bbbbbb", ".sbbbbbb", "..rrrrrr", "..bbbbbb", "...bb...", "...bb...",
        ],
        palette: &[(b'b', [58, 60, 80]), (b'r', [220, 45, 45]), (b'e', [20, 18, 24]), SKIN],
        eye: b'e',
    },
    Skin {
        name: "Vaquero",
        rows: [
            "........", "....hhhh", "...hhhhh", "...HHHHH", "hhhhhhhh", "..ssssss", "..ssksss", "..ssksss",
            "..ssssss", "...rrrrr", "..bbbbbb", ".sbbbbbb", "..HHHHHH", "..cccccc", "...cc...", "...HH...",
        ],
        palette: &[
            (b'h', [160, 105, 55]),
            (b'H', [90, 55, 30]),
            (b'r', [215, 55, 45]),
            (b'b', [220, 150, 70]),
            (b'c', [65, 95, 165]),
            SKIN,
            K,
        ],
        eye: b'k',
    },
    Skin {
        name: "Astronauta",
        rows: [
            "........", "........", "...wwwww", "..wwwwww", ".wwvvvvv", ".wvbvvvv", ".wvvevvv", ".wvvevvv",
            ".wwvvvvv", "..wwwwww", "..gwwwww", ".wwwwwwb", "..wwwwww", "..gggggg", "...ww...", "...gg...",
        ],
        palette: &[
            (b'w', [240, 240, 246]),
            (b'g', [165, 172, 190]),
            (b'v', [35, 55, 125]),
            (b'b', [120, 185, 255]),
            (b'e', [225, 240, 255]),
        ],
        eye: b'e',
    },
    Skin {
        name: "Fantasma de fuego",
        rows: FLAME,
        palette: &[(b'r', [85, 60, 215]), (b'o', [95, 145, 255]), (b'y', [185, 218, 255]), K],
        eye: b'k',
    },
    Skin {
        name: "Tiburón",
        rows: [
            "................",
            "................",
            "................",
            "................",
            "........d.......",
            ".......dd.......",
            "d......bbbbbb...",
            "dd...bbbbbbbbbb.",
            "ddbbbbbbbbbbkbbb",
            "ddbbbbbbbbbbbbbb",
            "dd.bbbbwwwkwkwkw",
            "d..bwwwwwwwwwww.",
            "....wwwwwwwwww..",
            ".....bd...bd....",
            ".....dd...dd....",
            ".....dd...dd....",
        ],
        palette: &[(b'b', [75, 145, 225]), (b'd', [45, 95, 165]), W, K],
        eye: b'k',
    },
    Skin {
        name: "Mago",
        rows: [
            ".......B", "......BB", ".....BBB", "....ByBB", "...BBBBB", ".BBBBBBB", "..ssssss", "..ssksss",
            "..wsksss", "..wwwwww", "..rwwwww", ".srrwwww", "..rrrrww", "..rrrrrr", "..rrrrrr", "..rrrrrr",
        ],
        palette: &[
            (b'B', [45, 75, 190]),
            (b'y', [250, 220, 80]),
            (b'r', [65, 100, 215]),
            (b'w', [240, 240, 240]),
            SKIN,
            K,
        ],
        eye: b'k',
    },
];
