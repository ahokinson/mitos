FROM gcr.io/oss-fuzz-base/base-builder-rust

COPY . $SRC/mitos
COPY ./.clusterfuzzlite/build.sh $SRC/build.sh
RUN chmod +x $SRC/build.sh
WORKDIR $SRC/mitos
