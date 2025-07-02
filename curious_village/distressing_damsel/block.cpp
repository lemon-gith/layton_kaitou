#include <iostream>


class Block{
public:
  Block(const char& type, unsigned short &base_x, unsigned short &base_y){
    throw std::logic_error("Derived Constructor not defined");
  }
private:
  //Coords base_location;
};