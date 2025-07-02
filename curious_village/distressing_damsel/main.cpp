#include <iostream>
#include <array>
#include <vector>

#include "block.h"


struct Coords{
  unsigned short x, y;
};

struct BlockLocation{
  char block_type;
  unsigned short base_x;
  unsigned short base_y;
};


struct Move{
  Block* block_to_move;  // refer directly to block for redundancy comparisons
  char move_direction;  // 'u', 'd', 'l', 'r'
};

struct MoveNode{
  Move current_move;
  std::vector<MoveNode*> next_moves;

  void printTree(){
    // TODO: define this recursive tree printing for the winning moves
  }
};


class PlayingField{
public:
  PlayingField(unsigned short &height, unsigned short &width){
    field.reserve(width);
    for(unsigned short i; i < width; i++){
      std::vector<Block*> column(height, nullptr);
      field.push_back(column);
    }
  }
  PlayingField(const PlayingField &init_field, Move &move_made){
    field.reserve(init_field.field.size());
    for (const std::vector<Block*> &column : init_field.field){
      field.push_back(column);
    }
    empty_gaps = init_field.empty_gaps;
    // TODO: make sure this runs as it should
  }

  // main recursive function, propagates game down chain
  MoveNode* propagate(){
    // TODO: make this run the way it should, building the tree
    // produce potential movelist
    // prune movelist
    // propagate movelist
    // return propagations
  }

  ~PlayingField(){
    // I feel like this needs a destructor, but I'm not absolutely sure
  }
private:
  std::vector<std::vector<Block*>> field;
  std::array<Coords, 2> empty_gaps;
};


int main(){
  // think this should give me a more easily modifiable shape, but still can't edit at runtime, only compile time
  unsigned short height, width;/*
  std::cout << "Please enter the height of the field: ";
  std::cin >> height;
  std::cout << "and the width: ";
  std::cin >> width;*/
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
