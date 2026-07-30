# The first item of a list or hash is left out of the alignment group its
# siblings form: the `=>` of every line but the first is padded to a common
# column, and the first is closed up. Already-aligned code comes back with one
# line changed, and the result is stable — the output is not aligned, and
# formatting it again leaves it that way.
is $result, +{
    alpha       => 1,
    bravo_bravo => 2,
    charlie     => 3,
};

call_it(
    alpha       => $a,
    bravo_bravo => $b,
    charlie     => $c,
);
