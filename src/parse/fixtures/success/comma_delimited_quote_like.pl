# A `,` after a quote-like keyword is its delimiter, never a separator. perl has
# no bareword exception here: `(q, 1)` is a fatal "can't find string terminator".
my $v = "path/";

$v .= "/" unless $v =~ m,/\z,,;
$v =~ s,/\*\z,,;
my @parts = split m,/,, $v;
print "yes\n" if $v =~ m,abc,;
$v =~ tr,a-z,A-Z,;

# The exceptions that do exist: a fat comma quotes the name, and a lone word in
# a hash subscript is a key.
my %h = (s => 1, q => 2, y => 3, tr => 4, m => 5);
my $one = $h{q};
my $two = $h{s};

# A block-taking list operator whose prototype we cannot see. The block reading
# is chosen because a term follows it, and a term cannot follow an expression.
my @words = qw(terse noopt);
my $key = "terse";
print "found\n" if any { $_ eq $key } qw(terse noopt nostrip);
my @big = grep { $_ > 1 } (1, 2, 3);
my $first = first { $_ > 1 } @big;

# ... and the readings it must not take: a hashref argument, and a subtraction.
my $ref = make_config { verbose => 1 };
my $count = counter {} - 1;
