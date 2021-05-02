# X in Y

Find long chains of 'place X in the bounday Y, place Y is in boundary Z' chains from OpenStreetMap

Run with 

There is a `place=city_block` called [New York (node 3,495,330,515)](https://www.openstreetmap.org/node/3495330515) in [Sweden (rel. 52,822)](https://www.openstreetmap.org/relation/52822) (`admin_level=2`), and there is a `place=hamlet` called [Sweden (node 151,467,606)](https://www.openstreetmap.org/node/151467606) in [Arkansas (rel. 161,646)](https://www.openstreetmap.org/relation/161646) (`admin_level=4`), and there is a `place=hamlet` called [Arkansas (node 157,545,126)](https://www.openstreetmap.org/node/157545126) in [West Virginia (rel. 162,068)](https://www.openstreetmap.org/relation/162068) (`admin_level=4`)o

I wonder how far we can go? Run `./make.sh FILENAME.osm.pbf`

# Results

As of May 2021, I have found a chain of 3,200 place/boundary pairs.

Copyright © 2021, GNU Affero GPL. Source code URL in [`Cargo.toml`](Cargo.toml).
