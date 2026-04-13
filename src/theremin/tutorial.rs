/// Song and scale data for the Theremin tutorial mode.

// ─── Types ────────────────────────────────────────────────────────────────────

pub struct TutorialNote {
    pub freq:  f32,
    pub beats: f32,
    pub label: &'static str,
}

pub struct TutorialSong {
    pub name:        &'static str,
    pub description: &'static str,
    pub bpm:         f32,
    pub notes:       &'static [TutorialNote],
}

pub struct TutorialScale {
    pub name:        &'static str,
    pub description: &'static str,
    pub notes:       &'static [(f32, &'static str)],
}

/// Active tutorial state.
pub enum TutorialMode {
    /// User practice: hold cursor on target note to advance.
    Song  { song_idx: usize, note_idx: usize, time_on: f32 },
    /// Computer plays the song automatically — user watches and listens.
    Autoplay {
        song_idx:      usize,
        note_idx:      usize,
        time_on_note:  f32,   // elapsed on current note
        cursor_x:      f32,   // animated screen X
        cursor_y:      f32,   // fixed screen Y
    },
    /// Free-play scale exploration.
    Scale(usize),
}

/// Seconds the cursor must stay on-target before the note registers.
pub const ADVANCE_SECS: f32 = 0.45;
/// Hit-detection radius in canvas pixels (≈ 1.5 semitones on a 1000 px canvas).
pub const TOLERANCE_PX: f32 = 35.0;

// ─── Note shorthand ───────────────────────────────────────────────────────────

macro_rules! n {
    ($f:expr, $b:expr, $l:expr) => { TutorialNote { freq: $f, beats: $b, label: $l } };
}

// ─── Songs ────────────────────────────────────────────────────────────────────

pub static SONGS: &[TutorialSong] = &[

    TutorialSong {
        name:        "Ode to Joy",
        description: "Beethoven · 9th Symphony · E major",
        bpm:         72.0,
        notes: &[
            // Phrase 1
            n!(329.63,1.0,"E4"), n!(329.63,1.0,"E4"), n!(369.99,1.0,"F#4"), n!(415.30,1.0,"G#4"),
            n!(415.30,1.0,"G#4"), n!(369.99,1.0,"F#4"), n!(329.63,1.0,"E4"), n!(311.13,1.0,"D#4"),
            n!(277.18,1.0,"C#4"), n!(277.18,1.0,"C#4"), n!(311.13,1.0,"D#4"), n!(329.63,1.0,"E4"),
            n!(329.63,1.5,"E4"), n!(311.13,0.5,"D#4"), n!(311.13,2.0,"D#4"),
            // Phrase 2
            n!(329.63,1.0,"E4"), n!(329.63,1.0,"E4"), n!(369.99,1.0,"F#4"), n!(415.30,1.0,"G#4"),
            n!(415.30,1.0,"G#4"), n!(369.99,1.0,"F#4"), n!(329.63,1.0,"E4"), n!(311.13,1.0,"D#4"),
            n!(277.18,1.0,"C#4"), n!(277.18,1.0,"C#4"), n!(311.13,1.0,"D#4"), n!(329.63,1.0,"E4"),
            n!(311.13,1.5,"D#4"), n!(277.18,0.5,"C#4"), n!(277.18,2.0,"C#4"),
        ],
    },

    TutorialSong {
        name:        "Somewhere Over the Rainbow",
        description: "Harold Arlen · Wizard of Oz · C major",
        bpm:         60.0,
        notes: &[
            n!(261.63,1.0,"C4"), n!(523.25,1.0,"C5"),                       // "Some-where"
            n!(493.88,0.5,"B4"), n!(392.00,0.5,"G4"), n!(329.63,1.0,"E4"),  // "o-ver the"
            n!(261.63,0.5,"C4"), n!(293.66,0.5,"D4"),                        // "rain-"
            n!(329.63,1.0,"E4"), n!(349.23,2.0,"F4"),                        // "-bow"
            n!(392.00,0.5,"G4"), n!(392.00,0.5,"G4"),                        // "way up"
            n!(349.23,1.0,"F4"), n!(329.63,1.0,"E4"),                        // "high"
            n!(293.66,0.5,"D4"), n!(261.63,2.0,"C4"),                        // "there's a"
            n!(293.66,0.5,"D4"), n!(329.63,0.5,"E4"),                        // "land"
            n!(349.23,0.5,"F4"), n!(392.00,0.5,"G4"),                        // "that I"
            n!(440.00,1.0,"A4"), n!(392.00,2.0,"G4"),                        // "heard of"
        ],
    },

    TutorialSong {
        name:        "Good Vibrations",
        description: "Beach Boys · theremin riff · Ab major",
        bpm:         96.0,
        notes: &[
            n!(466.16,0.5,"Bb4"), n!(415.30,0.5,"Ab4"), n!(369.99,0.5,"Gb4"), n!(311.13,1.0,"Eb4"),
            n!(466.16,0.5,"Bb4"), n!(415.30,0.5,"Ab4"), n!(369.99,0.5,"Gb4"), n!(311.13,1.5,"Eb4"),
            n!(369.99,0.5,"Gb4"), n!(415.30,0.5,"Ab4"), n!(466.16,0.5,"Bb4"), n!(622.25,1.5,"Eb5"),
            n!(554.37,0.5,"Db5"), n!(466.16,0.5,"Bb4"), n!(415.30,1.5,"Ab4"),
        ],
    },

    TutorialSong {
        name:        "Fur Elise",
        description: "Beethoven · A minor",
        bpm:         80.0,
        notes: &[
            // Main motif
            n!(659.26,0.5,"E5"), n!(622.25,0.5,"D#5"), n!(659.26,0.5,"E5"),
            n!(622.25,0.5,"D#5"), n!(659.26,0.5,"E5"),
            n!(493.88,0.5,"B4"), n!(587.33,0.5,"D5"), n!(523.25,0.5,"C5"), n!(440.00,2.0,"A4"),
            // Bridge
            n!(261.63,0.5,"C4"), n!(329.63,0.5,"E4"), n!(440.00,0.5,"A4"), n!(493.88,2.0,"B4"),
            n!(329.63,0.5,"E4"), n!(415.30,0.5,"G#4"), n!(493.88,0.5,"B4"), n!(523.25,2.0,"C5"),
            // Return
            n!(329.63,0.5,"E4"), n!(659.26,0.5,"E5"), n!(622.25,0.5,"D#5"),
            n!(659.26,0.5,"E5"), n!(622.25,0.5,"D#5"), n!(659.26,0.5,"E5"),
            n!(493.88,0.5,"B4"), n!(587.33,0.5,"D5"), n!(523.25,0.5,"C5"), n!(440.00,3.0,"A4"),
        ],
    },
];

// ─── Scales ───────────────────────────────────────────────────────────────────

pub static SCALES: &[TutorialScale] = &[

    TutorialScale {
        name:        "Pentatonic Minor",
        description: "5 notes — nothing sounds wrong",
        notes: &[
            (220.00,"A3"), (261.63,"C4"), (293.66,"D4"),
            (329.63,"E4"), (392.00,"G4"), (440.00,"A4"),
        ],
    },

    TutorialScale {
        name:        "Blues",
        description: "Add the b5 for soulful tension",
        notes: &[
            (220.00,"A3"), (261.63,"C4"), (293.66,"D4"),
            (311.13,"Eb4"), (329.63,"E4"), (392.00,"G4"), (440.00,"A4"),
        ],
    },

    TutorialScale {
        name:        "D Dorian",
        description: "Minor with raised 6th — jazz & folk",
        notes: &[
            (293.66,"D4"), (329.63,"E4"), (349.23,"F4"), (392.00,"G4"),
            (440.00,"A4"), (493.88,"B4"), (523.25,"C5"), (587.33,"D5"),
        ],
    },

    TutorialScale {
        name:        "C Major",
        description: "The foundation — all natural notes",
        notes: &[
            (261.63,"C4"), (293.66,"D4"), (329.63,"E4"), (349.23,"F4"),
            (392.00,"G4"), (440.00,"A4"), (493.88,"B4"), (523.25,"C5"),
        ],
    },

    TutorialScale {
        name:        "A Minor",
        description: "Natural minor — melancholic & expressive",
        notes: &[
            (220.00,"A3"), (246.94,"B3"), (261.63,"C4"), (293.66,"D4"),
            (329.63,"E4"), (349.23,"F4"), (392.00,"G4"), (440.00,"A4"),
        ],
    },
];
