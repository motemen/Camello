use feature 'signatures';

sub greet ($name, $greeting = "Hello") {
    "$greeting, $name!"
}

sub flexible ($first, $, $third //= 3, @rest) {
    return $first + @rest;
}

my $anon = sub ($value ||= 10, %opts) {
    return $opts{scale} ? $value * $opts{scale} : $value;
};
