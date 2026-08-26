# INDENT-4 places a block's contents and its closing brace from the line the
# construct owning it began on, which is not the statement's level once the
# writer has wrapped it. A bracket opened on a wrapped line asks the same
# question and gets a different answer: its contents and its closing bracket
# come back from the statement's level, shallower than the bracket holding them.
#
# Not new — `foo(`, `(` and a signature have always agreed on it, and the
# signature only joined them once it became a bracket like the others.
my $x = $a
    + foo(
    1,
    2
);

my $y = $a
    && (
    1,
    2
);

sub configure
(
$debug ||= 0,
%opts
)
{
    return 1;
}
