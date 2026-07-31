# `sub` before `=>` is a bareword key, not the start of an anonymous
# subroutine — the JWT `sub` claim is the common case.
my %claim = (
    iss => 'https://issuer.example',
    sub => 'subject-id',
);

my $token = +{ sub => 12345 };
print $claim{sub}, $token->{sub};
