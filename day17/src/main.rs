use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs;

const WIDTH: usize = 300;
const OFFSET: usize = 300;
const SPRING: (usize, usize) = (500, 0);
const N_ITERATIONS: usize = 120;

fn main() {
    let mut map = read_input("input.txt");
    println!("\x1b[2J");
    map.print();
    for _ in 0..N_ITERATIONS {
        map.tick();
        map.print();
    }

    let mut count_at_rest = 0;
    let mut count_hypothetical = 0;
    for y in map.ymin..=map.ymax {
        for x in 0..WIDTH {
            let tile = map.cells[y][x];
            if tile == '~' {
                count_at_rest += 1;
            } else if tile == '|' {
                count_hypothetical += 1;
            }
        }
    }
    println!("Water can reach {} tiles", count_at_rest + count_hypothetical);
    println!("There are {} water tiles at rest", count_at_rest);

}

fn read_input(filename: &str) -> Map {
    let input = fs::read_to_string(filename)
        .expect("Failed to read input");

    let mut clay_ranges = Vec::new();
    let mut ymin = std::usize::MAX;
    let mut ymax = 0;
    for line in input.lines() {
        let mut iter = line.splitn(2, ",");
        let first = iter.next().unwrap().trim();
        let second = iter.next().unwrap().trim();

        let mut iter = first.splitn(2, "=");
        let first_var = iter.next().unwrap().trim();
        let first_value: usize = iter.next().unwrap().trim().parse().unwrap();

        let mut iter = second.splitn(2, "=");
        let _second_var = iter.next().unwrap().trim();
        let second_value = iter.next().unwrap().trim();

        let mut iter = second_value.splitn(2, "..");
        let second_value_start: usize = iter.next().unwrap().trim().parse().unwrap();
        let second_value_end: usize = iter.next().unwrap().trim().parse().unwrap();

        let first_range = first_value..=first_value;
        let second_range = second_value_start..=second_value_end;
        if first_var == "x" {
            clay_ranges.push((first_range, second_range));
            ymin = ymin.min(second_value_start);
            ymax = ymax.max(second_value_end);
        } else {
            clay_ranges.push((second_range, first_range));
            ymin = ymin.min(first_value);
            ymax = ymax.max(first_value);
        }
    }

    let mut cells: Vec<[char; WIDTH]> = Vec::new();
    for _y in 0..=ymax {
        cells.push(['.'; WIDTH]);
    }

    for (rx, ry) in &clay_ranges {
        for y in ry.clone() {
            for x in rx.clone() {
                cells[y][x-OFFSET] = '#';
            }
        }
    }

    // Water spring
    cells[SPRING.1][SPRING.0-OFFSET] = '+';

    Map { cells: cells, ymin, ymax }
}

struct Map {
    cells: Vec<[char; WIDTH]>,
    ymin: usize,
    ymax: usize,
}

type Pos = (usize, usize);

impl Map {
    fn print(&self) {
        println!("\x1b[H");
        for (y, row) in self.cells.iter().enumerate() {
            if y > 110 {
                break;
            }
            print!("{:4}: ", y);
            for &cell in &row[..] {
                match cell {
                    '+' => print!("\x1b[31m{}\x1b[0m", cell),
                    '~' => print!("\x1b[34m{}\x1b[0m", cell),
                    '|' => print!("\x1b[36m{}\x1b[0m", cell),
                    _ => print!("{}", cell),
                }
            }
            println!();
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    fn tick(&mut self) {
        let mut seen = HashSet::new();
        let mut edge = VecDeque::new();

        edge.push_back(SPRING);

        while let Some(pos) = edge.pop_front() {
            seen.insert(pos);
            if self.get(pos) == '.' {
                self.set(pos, '|');
            }

            let new_tiles: Vec<Pos> = self.open_tiles(pos).into_iter().filter(|&p| ! seen.contains(&p)).collect();
            if new_tiles.is_empty() {
                // Fill
                let mut left = pos.0 - 1;
                while self.get((left, pos.1)) == '|' {
                    left -= 1;
                }
                let mut right = pos.0 + 1;
                while self.get((right, pos.1)) == '|' {
                    right += 1;
                }
                if self.get((left, pos.1)) == '#' && self.get((right, pos.1)) == '#' {
                    for x in left+1..right {
                        self.set((x, pos.1), '~')
                    }
                }
            }
            edge.extend(new_tiles);
        }
    }

    fn set(&mut self, pos: Pos, tile: char) {
        self.cells[pos.1][pos.0 - OFFSET] = tile;
    }

    fn get(&self, pos: Pos) -> char {
        self.cells[pos.1][pos.0 - OFFSET]
    }

    fn open_tiles(&self, pos: Pos) -> Vec<Pos> {
        let mut result = Vec::new();
        let below = below(pos);
        if self.valid(below) && ! occupied(self.get(below)) {
            result.push(below);
        } else if pos.1 < self.cells.len() - 1 {
            let left = left(pos);
            if self.valid(left) && ! occupied(self.get(left)) {
                result.push(left);
            }
            let right = right(pos);
            if self.valid(right) && ! occupied(self.get(right)) {
                result.push(right);
            }
        }

        result
    }

    fn valid(&self, pos: Pos) -> bool {
        OFFSET <= pos.0 && pos.0 < WIDTH + OFFSET && pos.1 < self.cells.len()
    }
}

fn below(pos: Pos) -> Pos {
    (pos.0, pos.1 + 1)
}

fn left(pos: Pos) -> Pos {
    (pos.0 - 1, pos.1)
}

fn right(pos: Pos) -> Pos {
    (pos.0 + 1, pos.1)
}

fn occupied(tile: char) -> bool {
    tile == '#' || tile == '~'
}

