// partitionfinder/capi.rs

use partitioner::Partitioner;
use std::mem;
use std::slice;
use {Bezieroid, ControlPoints, Endpoint, Subpath};

#[no_mangle]
pub unsafe extern fn pf_partitioner_new(endpoints: *const Endpoint,
                                        endpoint_count: u32,
                                        control_points: *const ControlPoints,
                                        control_points_count: u32,
                                        subpaths: *const Subpath,
                                        subpath_count: u32)
                                        -> *mut Partitioner<'static> {
    let mut partitioner =
        Box::new(Partitioner::new(slice::from_raw_parts(endpoints, endpoint_count as usize),
                                  slice::from_raw_parts(control_points,
                                                        control_points_count as usize),
                                  slice::from_raw_parts(subpaths, subpath_count as usize)));
    let partitioner_ptr: *mut Partitioner<'static> = &mut *partitioner;
    mem::forget(partitioner);
    partitioner_ptr
}

#[no_mangle]
pub unsafe extern fn pf_partitioner_destroy<'a>(partitioner: *mut Partitioner<'a>) {
    drop(mem::transmute::<*mut Partitioner<'a>, Box<Partitioner>>(partitioner))
}

#[no_mangle]
pub unsafe extern fn pf_partitioner_reset<'a>(partitioner: *mut Partitioner<'a>,
                                              new_endpoints: *const Endpoint,
                                              new_endpoint_count: u32,
                                              new_control_points: *const ControlPoints,
                                              new_control_points_count: u32,
                                              new_subpaths: *const Subpath,
                                              new_subpath_count: u32) {
    (*partitioner).reset(slice::from_raw_parts(new_endpoints, new_endpoint_count as usize),
                         slice::from_raw_parts(new_control_points,
                                               new_control_points_count as usize),
                         slice::from_raw_parts(new_subpaths, new_subpath_count as usize))
}

#[no_mangle]
pub unsafe extern fn pf_partitioner_partition<'a>(partitioner: *mut Partitioner<'a>) {
    (*partitioner).partition()
}

#[no_mangle]
pub unsafe extern fn pf_partitioner_bezieroids<'a>(partitioner: *mut Partitioner<'a>,
                                                   out_bezieroid_count: *mut u32)
                                                   -> *const Bezieroid {
    let bezieroids = (*partitioner).bezieroids();
    if !out_bezieroid_count.is_null() {
        *out_bezieroid_count = bezieroids.len() as u32
    }
    bezieroids.as_ptr()
}
