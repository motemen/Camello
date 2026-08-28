package App;
use strict;
use warnings;

use App::Page;
use App::Plain;
use App::Host;

sub run {
    my ($class) = @_;

    # `App::Page` called a generator at file scope, so it may have any method.
    my $page = App::Page->new;
    $page->title;
    $page->anything_at_all;

    # `App::Plain` did not, so its method set is the one its file declares —
    # plus what `App::Named` exports into it.
    my $plain = App::Plain->new;
    $plain->greet;
    $plain->shout;
    $plain->missing;
    #~ warning unknown-method: declares no method `missing`

    # `App::Host` imported a list nobody can read.
    my $host = App::Host->new;
    $host->render;
    $host->anything_at_all;

    return;
}

1;
