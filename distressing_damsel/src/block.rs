use std::collections::HashSet;
use crate::map::Coords;
/*
// just to retain logic, for now
// TODO: rewrite these to fit new Block logic
struct Block{
  char block_type;  // '1', '2', 'H', 'V', etc.
  unsigned short size_x;
  unsigned short size_y;
};

struct BlockLocation{
  char block_type;
  unsigned short base_x;
  unsigned short base_y;
};
*/
pub struct Block {
/* TODO: define constructor
    pub Block(const char& type, unsigned short &base_x, unsigned short &base_y){

    }
*/
    john: HashSet<Coords>
}

pub fn test(john: u8) {
    println!("Greetings, I am {john}");
}