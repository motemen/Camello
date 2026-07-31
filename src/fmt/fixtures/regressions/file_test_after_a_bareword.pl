# `-s` and `-y` are file tests, and `s` and `y` are also quote-like operators.
# After a bareword the `-` reads as subtraction, so the letter is taken for the
# operator and its body runs to the end of the file. The same test parses in
# every other position, including after a builtin like `print`.
sub ok;

sub foo {
    my ($path) = @_;
    ok -f $path, 'a file test whose letter is not a quote-like operator';
    ok((-s $path) > 0, 'parenthesised');
    print -s $path;
    my $size = -s $path;
    return $size;
}

sub bar {
    my ($path) = @_;
    ok -s $path > 0, 'the case that does not parse';
}
