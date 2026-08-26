use utf8;
use strict;
use warnings;

# `eval` was absent from the builtin table, so the parser fell through to
# "unknown name, expect an operator next", `<<EOT` lexed as a left shift, and
# the heredoc body was promoted to code.
my $document = eval <<'EOT';
this is data, not code
EOT

# `eval BLOCK` is a block, never an anonymous hash — perl reads the brace the
# same way.
my $result = eval { 1 + 1 };
eval { risky() } or warn "failed: $@";
my $computed = eval "1 + 1";

# `wantarray` takes no arguments; giving it a list shape let it swallow what
# came after.
sub context {
    return wantarray ? (1, 2) : 1;
}

# Named unary operators bind one operand, tighter than comparison.
my @lines = ("a\n");
chomp @lines;
my $size = length $lines[0];
my $kind = ref \@lines;

# Under `use utf8` a word character is a word character. Scanning identifiers as
# ASCII did not reject these names, it split them: `my $café = 1;` came out as
# `my $caf é = 1;`.
my $café = "au lait";
my %メニュー = (コーヒー => $café);

sub 名前 {
    my ($引数) = @_;
    return $引数;
}

print 名前($café), "\n";
print $メニュー{コーヒー}, "\n";
