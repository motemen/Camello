print STDOUT"hello";
print(STDOUT"world");
print   STDOUT    "line\n";

map {$_}@list;
map({$_}@list);
map {$_}(1,2,3);
map {$_}[1,2,3];

grep{$_}@list;
grep({$_}@list);
sort({$a<=>$b} @list);

print { $fh } "hello";
print( {$fh} map { ref eq 'ARRAY' ? @$_ : $_ } @data );
print($fh 'do{ my ' . $dump . '}');
printf($fh "%s\n", $line);

# The indirect-object form of `$fh->autoflush(1)`
use IO::Handle;
open my $fh, '>', $path or die $!;
autoflush $fh 1;

# A bareword with no declaration in sight, taking a hash or a glob
sub _error;
sub getdata;
sub handler;
_error %state, $cb, { @pseudo };
getdata %^H, $wiz;
handler &$callback, 1;

# A named unary with no argument, followed by a word operator
my @names = grep { defined and !ref and /^\w+$/s } @spec;
my $plain = ref or 1;

each%$hash;
each @array;
values%$hash;
values @array;
