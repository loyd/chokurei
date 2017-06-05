use std::str;
use std::fs::File;
use std::io::Read;
use std::collections::{HashMap, HashSet};

use rust_stemmers::{Algorithm, Stemmer};

use common::messages::Entry;

// TODO: pass via environment or config.
const DICTIONARY_FILE: &str = "dictionary.txt";

lazy_static! {
    static ref STOP_WORDS: HashSet<&'static str> = {
        include_str!("stopwords.txt")
            .split_whitespace()
            .collect()
    };

    static ref RU_STEMMER: Stemmer = Stemmer::create(Algorithm::Russian);
    static ref EN_STEMMER: Stemmer = Stemmer::create(Algorithm::English);

    static ref DICTIONARY_DATA: String = {
        let mut file = File::open(DICTIONARY_FILE).unwrap();

        let mut data = String::new();
        file.read_to_string(&mut data).unwrap();

        data
    };

    // TODO: rebuild dictionary.
    static ref DICTIONARY: HashMap<&'static str, u32> = {
        DICTIONARY_DATA.split_whitespace()
            .zip(0u32..)
            .collect()
    };
}

pub struct Document {
    entry: Entry,
    vector: Vector,
}

pub struct Vector(Vec<(u32, f32)>);

impl Document {
    pub fn from_entry(entry: Entry) -> Option<Document> {
        let vector = vectorize(&entry.content);

        match vector.0.len() {
            0 => None,
            _ => Some(Document { entry, vector })
        }
    }

    pub fn entry(&self) -> &Entry {
        &self.entry
    }

    pub fn distance(&self, that: &Document) -> f32 {
        if that.vector.0.len() < self.vector.0.len() {
            return that.distance(self);
        }

        let dist = distance(&self.vector, &that.vector);

        // [-1, +1] --> [0, +1]
        let measure = 0.5 * (1. + dist);

        measure * (1. - penalty(&self.entry, &that.entry))
    }
}

fn vectorize(content: &str) -> Vector {
    let word_it = content.split_whitespace()
        .map(str::to_lowercase)
        .filter(is_not_stopword)
        .map(stem_word)
        .filter_map(get_word_index);

    let counter = count_words(word_it);

    produce_vector(counter)
}

fn is_not_stopword(word: &String) -> bool {
    !STOP_WORDS.contains(word as &str)
}

fn stem_word(word: String) -> String {
    let word = RU_STEMMER.stem(&word);
    let word = EN_STEMMER.stem(&word);

    word.into_owned()
}

fn get_word_index(word: String) -> Option<u32> {
    DICTIONARY.get(&word as &str).cloned()
}

fn count_words<I: Iterator<Item=u32>>(words: I) -> HashMap<u32, u32> {
    let mut counter = HashMap::new();

    for word in words {
        let count = counter.entry(word).or_insert(0);
        *count += 1;
    }

    counter
}

fn produce_vector(counter: HashMap<u32, u32>) -> Vector {
    let length = counter.values().sum::<u32>() as f32;

    let mut vector = Vec::new();
    let mut magnitude = 0.;

    // Calculate TF and the magnitude.
    for (index, count) in counter {
        let tf = count as f32 / length;

        // TODO: tf-idf instead tf.
        vector.push((index, tf));

        magnitude += tf * tf;
    }

    magnitude = magnitude.sqrt();

    // Normalize the vector.
    for item in vector.iter_mut() {
        item.1 /= magnitude;
    }

    vector.sort_unstable_by_key(|item| item.0);

    Vector(vector)
}

fn distance(lhs: &Vector, rhs: &Vector) -> f32 {
    let (a, b) = (&lhs.0, &rhs.0);

    let max_j = b.len();
    let mut j = 0;

    let mut distance = 0.;

    for &(s_idx, s_val) in a {
        while j < max_j && b[j].0 < s_idx {
            j += 1;
        }

        if j == max_j {
            break;
        }

        if b[j].0 == s_idx {
            distance += b[j].1 * s_val;
        }
    }

    distance
}

fn penalty(lhs: &Entry, rhs: &Entry) -> f32 {
    let mut penalty = 0.;

    if lhs.source == rhs.source {
        penalty += 0.3;
    }

    /*  penalty
     *     ^
     *     |
     * 0.5 +                           ======
     *     |                     =======
     *     |               ======
     *     |         ======
     *   0 +========|------------------|-----> delta
     *              2d                  7d
     */
    let delta = (lhs.published.sec - rhs.published.sec).abs() as f32 / (24. * 60.);

    if delta > 2. {
        penalty += (0.1 * delta - 0.2).min(0.5);
    }

    assert!(penalty < 1.0);

    penalty
}

#[test]
fn it_calculates_distance() {
    fn test(a: Vec<(u32, f32)>, b: Vec<(u32, f32)>, result: f32) {
        let a = Vector(a);
        let b = Vector(b);

        assert_eq!(distance(&a, &b), result);
        assert_eq!(distance(&b, &a), result);
    }

    test(
        vec![(1, 2.), (2, 3.), (5, 3.), (10, 1.), (12, 2.)],
        vec![(2, 1.), (10, 2.), (20, 5.)],
        5.
    );

    test(
        vec![(2, 3.), (20, 1.)],
        vec![(2, 1.), (10, 2.), (20, 5.)],
        8.
    );
}
