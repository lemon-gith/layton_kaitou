
pub struct Coords {
  x: u32,
  y: u32
}

pub struct Map {

}

// TODO: what to make of this code?
/*

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

*/
