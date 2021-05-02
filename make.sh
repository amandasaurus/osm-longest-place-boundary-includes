#! /bin/bash
set -o errexit -o nounset -o pipefail

ROOT=$(realpath $(dirname $0))
INPUTFILE=${1:?Arg 1 must be an OSM file}
INPUTFILE=$(realpath "$INPUTFILE")
OUTPUTDIR=${2:-.}
cd "$OUTPUTDIR"

#pyosmium-up-to-date -vv "$INPUTFILE" || true

if [ "$INPUTFILE" -nt place.osm.pbf ] ; then
	echo "Extracting place nodes..."
	osmium tags-filter --overwrite "$INPUTFILE" -o place.osm.pbf n/place
fi

if [ "$INPUTFILE" -nt admin_level.osm.pbf ] ; then
	echo "Extracting admin_levels..."
	osmium tags-filter --overwrite "$INPUTFILE" -o admin_level.osm.pbf admin_level
fi

if [ place.osm.pbf -nt .place_imported ] ; then
	echo "Importing place nodes..."
	osm2pgsql -l -S x-in-y.style --slim --drop -p place place.osm.pbf
	for T in line polygon roads ; do
		psql -c "drop table place_$T"
	done
	psql -c "create index place_point__place on place_point (place)"
	psql -c "analyze place_point;"
	touch .place_imported
fi

if [ admin_level.osm.pbf -nt .admin_level_imported ] ; then
	echo "Importing admin_levels..."
	osm2pgsql -l -S x-in-y.style --slim --drop -p admin_level admin_level.osm.pbf
	for T in line point roads ; do
		psql -c "drop table admin_level_$T"
	done
	psql -c "create index admin_level_polygon__admin_level on admin_level_polygon (admin_level)"
	psql -c "analyze admin_level_polygon;"
	touch .admin_level_imported
fi

if [ .place_imported -nt place-in-area.csv.gz ] || [ .admin_level_imported -nt place-in-area.csv.gz ] || [ $0 -nt place-in-area.csv.gz ] ; then

	psql -c "COPY (
		select
				'n' as place_osmtype,
				place.osm_id as place_id,
				coalesce(place.\"name:en\", place.name) as place_name,
				place.place as place_type,
				st_y(place.way) as place_lat,
				st_x(place.way) as place_lon,
				(case when boundary.osm_id<0 then 'r' else 'w' end) as boundary_osmtype,
				abs(boundary.osm_id) as boundary_id,
				coalesce(boundary.\"name:en\", boundary.name) as boundary_name,
				boundary.admin_level as boundary_admin_level
			from
				place_point as place
				JOIN admin_level_polygon as boundary
					ON (
						boundary.way && place.way
						AND ST_Contains(boundary.way, place.way)
					)
		) TO STDOUT WITH ( FORMAT CSV, HEADER )" \
			| pv -l -N "calculating place/boundary combos" \
			| gzip \
			> place-in-area.csv.gz.tmp

	mv place-in-area.csv.gz.tmp place-in-area.csv.gz


fi

cd $ROOT
exec cargo +nightly run --release -- place-in-area.csv.gz distances.md
