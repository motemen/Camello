# The first `=>` of an argument list written on one line is pulled into the
# alignment group the surrounding statements form, and padded to a column that
# has nothing to do with anything inside the call. The second `=>` on the same
# line is left alone.
sub t {
    my $one   = foo(alpha => $x);
    my $two   = bar(b => $y, charlie => $z);
    my $three = 1;
}
