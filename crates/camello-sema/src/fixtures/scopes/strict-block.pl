# `use strict` ends with the block it is in, as perl's lexical pragmas do.
{
    package First;
    use strict;
    print $inside;    #~ error undeclared-variable: `$inside`
}

{
    package Second;
    $size = 0;
    use strict;
}
