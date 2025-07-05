UPDATE finished_nightly SET mode = 'std' WHERE mode = 'miri-std';

UPDATE build_info SET mode = 'std' WHERE mode = 'miri-std';
