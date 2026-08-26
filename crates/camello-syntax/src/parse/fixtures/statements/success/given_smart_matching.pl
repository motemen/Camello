use v5.14;
given($item) {
    when (undef) { say "Item is undefined"; }
    when ([1, 3, 5]) { say "Item is 1, 3, or 5"; }
    when (qr/^[a-z]+$/) { say "Item consists of lowercase letters"; }
}
