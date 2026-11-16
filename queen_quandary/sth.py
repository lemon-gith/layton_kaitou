# check line by line using dynamic programming, kinda
# top line only need to check half, rest is all symmetric

# kill branches off when can, try not to build when shouldn't
# use column number for that check and complementary y = mx+c for lines


#TODO: Maybe turn this into an IronPython test project, use C++ for the core functionality and Python for the surrounding stuff, maybe then HTML, CSS, etc. for the visualisation, idk :)

#TODO: for later, can add blocking pieces


def letter_range(range_length):
  if range_length <= 0:
    raise ValueError(f"{range_length} is an invalid value to count to")

  if range_length == 1:
    return 'a'

  if range_length <= 26:
    for i in range(range_length):
      yield chr(ord('a') + i)
  else:
    end_letter = ""
    while range_length > 26:
      rem, range_length = divmod(range_length, 26)
      end_letter += chr(rem)

    current_letter = 'a'
    while current_letter < end_letter:
      if current_letter[-1] == 'z':
        if all(letter == 'z' for letter in current_letter):
          current_letter = "a"*(len(current_letter) + 1)
        else:
          #TODO: implement 'z' -> 'a' backpropagation
          pass
      else:
        current_letter[-1] = chr(ord(current_letter[-1]) + 1)

      yield current_letter


def line_recurse(solutions, current_solution, depth=1):
  #TODO: implement line recursion, using letter_range() and notes above
  pass


def place_queens(number):
  print(f"Attempting to place {number} queens on this {number}x{number} board", end="")

  if number == 1:
    return [["a1"]]

  solutions = []  # to be filled with solutions
  starting_depth = '1'
  from math import ceil
  for column in letter_range(ceil(number/2)):
    line_recurse(solutions, [column + starting_depth], 2)
    print(".", end="")

  print(f"\nand done, with {len(solutions)} solutions :D")
  return solutions


def get_params():
  """
  gets parameters as inputs from user, extracted due to extra sanitisation steps
  """
  width = height = 0
  while not (width and height):
    dim = input("What (2D) dimension of board would you like to work with? (e.g. 8x8)\n")

    if dim == "exit":
      exit()

    try:
      width, height = map(int, dim.split('x'))
    except ValueError:
      width = height = 0
      print("One of the entered values is not a parseable integer, please try again.")
      continue

    invalid_msg = ""
    if width <= 0:
      width = 0
      invalid_msg += "Your entered value of width is invalid.\n"
    if height <= 0:
      height = 0
      invalid_msg += "And your " if invalid_msg else "Your "
      invalid_msg += "entered value of height is invalid.\n"

    if invalid_msg:
      print(invalid_msg, end="")

  number = 0
  while not number:
    try:
      number = int(input(f"How many queens would you like to place on your {width}x{height} board?\n"))
    except ValueError:
      number = 0
      print("That was not a parseable integer, please try again.")
      continue

    if number <= 0:
      number = 0
      print("That is an invalid number of queens to place.")

    if number > min(width, height):
      number = 0
      print("That many queens cannot be placed on the specified board in accordance with the rules.")

  return (width, height, number)


def main():
  width, height, no_of_queens = get_params()
  print("Parameters received :)")

  if width == height and width == no_of_queens:
    solutions = place_queens(no_of_queens)
  else:
    print("Right now, will just return solutions for a minimal square, since other solutions are superfluous.")
    solutions = place_queens(no_of_queens)

  print(f"{len(solutions)} solutions in total:")
  for solution in solutions:
    print(solution)

  print("終わり :)")


if __name__ == "__main__":
  main()
