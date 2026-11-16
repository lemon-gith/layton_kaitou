/// logic relating to the behaviour of the blocks
mod block;
/// components responsible for laying out and managing the playing field
mod map;
/// components that interpret and translate business logic into fun shapes
mod ui;


use map::{Coords, Map};
use ui::moves::Move;
/*
 - Terminal-based: TUI (ratatui)?
 - quick select block using digit/char keys, move using arrow keys
 - - if you want to move it yourself
 - - game should be playable and settable before solved
 - otherwise, solver runs in the background
 - - left and right keys step through solution
 - - up and down keys step through different sols
 - try to make the interfacing bit modular, might try to upload to website
 - - try getting it working with TUI and with GUI, too
 */


fn main() {
    println!("Hello, world!");
    block::test(2);
}

// TODO: try to see what overarching call routine you want
/*

int main(){
  // think this should give me a more easily modifiable shape, but still can't edit at runtime, only compile time
  unsigned short height, width;/*
  std::cout << "Please enter the height of the field: ";
  std::cin >> height;
  std::cout << "and the width: ";
  std::cin >> width; */
  height = 4;
  width = 5;
  PlayingField starting_field = PlayingField(height, width);

  // TODO: take in list of block placements, possibly from UI

  MoveNode* winning_moves = starting_field.propagate();

  std::cout << "the winning move chains are: " << std::endl;
  winning_moves->printTree();

  // for the empty spaces generate movelist
  // only possible moves,
  // remove the moves that we don't want
  // kill boards with now empty movelists,
  // generate next board for recursive continuation

  std::cout << "hewwo" << std::endl;
}
 */
