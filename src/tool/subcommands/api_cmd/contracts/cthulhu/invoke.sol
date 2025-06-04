// SPDX-License-Identifier: MIT
pragma solidity = 0.8.30;

contract InvokeCthulhu {
  bool public cthulhu_is_here = false;
  function incoming_doom() public {
    cthulhu_is_here = true;
  }
}
