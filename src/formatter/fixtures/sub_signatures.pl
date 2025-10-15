# Subroutine signatures formatting samples
sub greet   ($name,$greeting="Hello"){
"$greeting, $name!"
}

sub flexible($first,$,$third //= 3,@rest){
    return $first + @rest;
}

sub configure
(
$debug ||= 0,
%opts
)
{
    return $opts{level} // $debug;
}

sub placeholder_only($,@,%) {
    return @_;
}

sub placeholder_default($thing,$=1) {
    return $thing + $;
}

my $anon = sub($value ||= 10,%opts){
    return $opts{scale}?$value*$opts{scale}:$value;
};

sub multiline(
    $alpha,
    $beta ||= compute_default(),
    %extra,
){
    return $alpha + $beta + scalar keys %extra;
}
