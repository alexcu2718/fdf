use slimmer_box::SlimmerBox;



pub type OsBytes = SlimmerBox<[u8], u16>; //10 bytes,this is basically a box with a much thinner pointer, it's 10 bytes instead of 16.

