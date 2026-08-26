# The first argument of a parenthesis-less list operator is a bareword that
# happens to be a keyword. `=>` quotes it, so it is a string in every case;
# `run(until => 1)` already parses, and only the unparenthesised form does not.
sub run;

run until   => 1;
run given   => 2;
run default => 3;
run if      => 4;
run else    => 5;
run package => 6;
