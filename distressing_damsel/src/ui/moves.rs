use crate::block::Block;
use crate::map::{Coords};


pub enum MoveDir {
    Up,
    Down,
    Left,
    Right
}

impl MoveDir {
    pub fn make_move(mdir: MoveDir, block: Block) {

    }
}

pub struct Move {
  block: Block,  // refer directly to block for redundancy comparisons
  dir: MoveDir  // 'u', 'd', 'l', 'r'
}

struct MoveNode{
    current_move: Move,
    next_moves: Vec<MoveNode>,
}

impl MoveNode {
    pub fn print_movetree(indent: isize){

        // TODO: define this recursive tree printing for the winning moves
    }
}