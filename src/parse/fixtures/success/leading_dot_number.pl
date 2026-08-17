my $half = .5;
my $bias = -.01;
my %opt = (wait => .1, retries => 1);
my $scaled = $max * -.1;
my $sum = .5 + .5;
substr $text, $max * -.1, $max * .1;

# A `.` where an operator is expected is still concatenation.
my $joined = $a . 5;
my $range = .5 .. 1.5;
