use std::collections::BTreeSet;

use loom::sync::{Arc, Mutex};
use loom::thread;
use rstest::rstest;

#[rstest]
#[case::write_new(4)]
#[case::write_existing(1)]
fn simulate_existing_clean_behavior(#[case] value_to_write: i32) {
    loom::model(move || {
        // Start out with `1` already as garbage.
        let tags = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([2, 3])));
        let objects = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([1, 2, 3])));

        let writer_tags = Arc::clone(&tags);
        let writer_objects = Arc::clone(&objects);
        let writer_thread = thread::spawn(move || {
            // This simulates the current behavior of writing objects where
            // the underlying objects are written first and the parent objects
            // written second, e.g., blob first and manifest later.
            let mut lock = writer_objects.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            let mut lock = writer_tags.lock().unwrap();
            lock.insert(value_to_write);
        });

        let cleaner_tags = Arc::clone(&tags);
        let cleaner_objects = Arc::clone(&objects);
        let cleaner_thread = thread::spawn(move || {
            let lock = cleaner_objects.lock().unwrap();
            let objects_snapshot = (*lock).clone();
            drop(lock);
            let lock = cleaner_tags.lock().unwrap();
            let tags_snapshot = (*lock).clone();
            drop(lock);

            let garbage_objects = objects_snapshot.difference(&tags_snapshot);
            for obj in garbage_objects {
                let mut lock = cleaner_objects.lock().unwrap();
                lock.remove(obj);
            }
        });

        cleaner_thread.join().unwrap();
        writer_thread.join().unwrap();

        let lock = objects.lock().unwrap();
        assert_eq!(*lock, BTreeSet::<i32>::from_iter([value_to_write, 2, 3]));
    });
}

#[rstest]
#[case::write_new(4)]
#[case::write_existing(1)]
fn simulate_proposed_clean_behavior(#[case] value_to_write: i32) {
    loom::model(move || {
        // Start out with `1` already as garbage.
        let tags = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([2, 3])));
        let objects = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([1, 2, 3])));
        let staged = Arc::new(Mutex::new(BTreeSet::<i32>::new()));

        let writer_tags = Arc::clone(&tags);
        let writer_objects = Arc::clone(&objects);
        let writer_staged = Arc::clone(&staged);
        let writer_thread = thread::spawn(move || {
            // Before writing any objects, a writer must create a hard reference
            // to them by "staging them":
            let mut lock = writer_staged.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            // Now it is safe to add to objects.
            let mut lock = writer_objects.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            let mut lock = writer_tags.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            // With the tag written, it is now safe to unstage the object.
            let mut lock = writer_staged.lock().unwrap();
            lock.remove(&value_to_write);
        });

        let cleaner_tags = Arc::clone(&tags);
        let cleaner_objects = Arc::clone(&objects);
        let cleaner_staged = Arc::clone(&staged);
        let cleaner_thread = thread::spawn(move || {
            let lock = cleaner_objects.lock().unwrap();
            let objects_snapshot = (*lock).clone();
            drop(lock);
            // We hold the "staged" lock while deleting objects so anything
            // trying to stage something has to wait.
            let staged_lock = cleaner_staged.lock().unwrap();
            {
                // We also hold the "staged" lock while reading the tags to
                // prevent the interleaving where:
                //   - in thread 1: 4 hasn't been added to tags yet
                //   - in thread 2: tags snapshot is captured (without 4)
                //   - in thread 1: 4 is added to tags and removed from staged
                //   - in thread 2: staged is locked but is empty, allowing 4
                //                  to be deleted
                let lock = cleaner_tags.lock().unwrap();
                let tags_snapshot = (*lock).clone();
                drop(lock);
                let garbage_objects = objects_snapshot
                    .difference(&tags_snapshot)
                    .copied()
                    .collect::<BTreeSet<_>>();
                // We're not allowed to delete anything that is staged.
                let to_delete = garbage_objects.difference(&staged_lock);
                for obj in to_delete {
                    let mut lock = cleaner_objects.lock().unwrap();
                    lock.remove(obj);
                }
            }
        });

        cleaner_thread.join().unwrap();
        writer_thread.join().unwrap();

        let lock = objects.lock().unwrap();
        assert_eq!(*lock, BTreeSet::<i32>::from_iter([value_to_write, 2, 3]));
    });
}

#[rstest]
#[case::write_new(4)]
#[case::write_existing(1)]
fn simulate_proposed_clean_behavior_version_2(#[case] value_to_write: i32) {
    loom::model(move || {
        // Start out with `1` already as garbage.
        let tags = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([2, 3])));
        let objects = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([1, 2, 3])));
        let staged = Arc::new(Mutex::new(BTreeSet::<i32>::new()));

        let writer_tags = Arc::clone(&tags);
        let writer_objects = Arc::clone(&objects);
        let writer_staged = Arc::clone(&staged);
        let writer_thread = thread::spawn(move || {
            // Before writing any objects, a writer must create a hard reference
            // to them by "staging them":
            let mut lock = writer_staged.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            // Now it is safe to add to objects.
            let mut lock = writer_objects.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            let mut lock = writer_tags.lock().unwrap();
            lock.insert(value_to_write);
            // In this behavior, 4 is not unstaged here. In practice, it would
            // be marked as "committed" and pruned later to give a reasonable
            // amount of time to elapse to avoid races. Any new writers of the
            // same object would add a new entry to staged to keep it alive
            // longer.
        });

        let cleaner_tags = Arc::clone(&tags);
        let cleaner_objects = Arc::clone(&objects);
        let cleaner_staged = Arc::clone(&staged);
        let cleaner_thread = thread::spawn(move || {
            let lock = cleaner_objects.lock().unwrap();
            let objects_snapshot = (*lock).clone();
            drop(lock);
            let lock = cleaner_tags.lock().unwrap();
            let tags_snapshot = (*lock).clone();
            drop(lock);
            let garbage_objects = objects_snapshot
                .difference(&tags_snapshot)
                .copied()
                .collect::<BTreeSet<_>>();
            // We hold the "staged" lock while deleting objects so anything
            // trying to stage something has to wait.
            let staged_lock = cleaner_staged.lock().unwrap();
            {
                // We're not allowed to delete anything that is staged.
                let to_delete = garbage_objects.difference(&staged_lock);
                for obj in to_delete {
                    let mut lock = cleaner_objects.lock().unwrap();
                    lock.remove(obj);
                }
            }
        });

        cleaner_thread.join().unwrap();
        writer_thread.join().unwrap();

        let lock = objects.lock().unwrap();
        assert_eq!(*lock, BTreeSet::<i32>::from_iter([value_to_write, 2, 3]));
    });
}

#[rstest]
#[case::write_new(4)]
#[case::write_existing(1)]
fn simulate_proposed_lock_free_clean_behavior(
    #[case] value_to_write: i32,
    #[values(true, false)] tags_first: bool,
) {
    loom::model(move || {
        // Start out with `1` already as garbage.
        let tags = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([2, 3])));
        let objects = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([1, 2, 3])));
        let staged = Arc::new(Mutex::new(BTreeSet::<i32>::new()));

        let writer_tags = Arc::clone(&tags);
        let writer_objects = Arc::clone(&objects);
        let writer_staged = Arc::clone(&staged);
        let writer_thread = thread::spawn(move || {
            // Before writing any objects, a writer must create a hard reference
            // to them by "staging them":
            let mut lock = writer_staged.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            // Now it is safe to add to objects.
            let mut lock = writer_objects.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            let mut lock = writer_tags.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            // With the tag written, it is now safe to unstage the object.
            let mut lock = writer_staged.lock().unwrap();
            lock.remove(&value_to_write);
        });

        let cleaner_tags = Arc::clone(&tags);
        let cleaner_objects = Arc::clone(&objects);
        let cleaner_staged = Arc::clone(&staged);
        let cleaner_thread = thread::spawn(move || {
            // This version doesn't have a global lock while deleting objects.
            // It makes two passes on the objects. The first pass determines
            // which objects may be garbage. The second pass verifies all
            // objects in the first list are still garbage.

            // Simulate reading tags and objects in either order.
            // Running two test variants is vastly faster than using threads to
            // simulate the order being non-deterministic.
            let (objects_snapshot, tags_snapshot) = if tags_first {
                let lock = cleaner_tags.lock().unwrap();
                let _tags_snapshot = (*lock).clone();
                drop(lock);
                let lock = cleaner_objects.lock().unwrap();
                let _objects_snapshot = (*lock).clone();
                drop(lock);
                (_objects_snapshot, _tags_snapshot)
            } else {
                let lock = cleaner_objects.lock().unwrap();
                let _objects_snapshot = (*lock).clone();
                drop(lock);
                let lock = cleaner_tags.lock().unwrap();
                let _tags_snapshot = (*lock).clone();
                drop(lock);
                (_objects_snapshot, _tags_snapshot)
            };

            let to_delete = objects_snapshot
                .difference(&tags_snapshot)
                .copied()
                .collect::<BTreeSet<_>>();

            // Disqualify any objects that (still) have a staging file.
            let staged_snapshot = cleaner_staged.lock().unwrap().clone();
            let to_delete = to_delete
                .difference(&staged_snapshot)
                .copied()
                .collect::<BTreeSet<_>>();
            // Finally, re-check the tags to ensure nothing new has been added
            // since we took our first snapshot.
            //
            // The following claim is only true when writing a new object. It is
            // not true if a writer is introducing a new hard reference to an
            // existing object that is currently considered garbage.
            //
            // It is impossible to not see either the tag or the staging file
            // for any non-garbage object. Since the tag is written before
            // deleting the staging file, and the staging file is written before
            // the object, if we know about the object then one of the two must
            // exist. We can't check for both in the same pass because we could
            // end up not seeing a just-written tag and miss seeing the just-
            // deleted staging file. In practice we only need to walk manifests
            // that were not seen in the original pass.
            let tags_2nd_snapshot = cleaner_tags.lock().unwrap().clone();
            let to_delete = to_delete
                .difference(&tags_2nd_snapshot)
                .copied()
                .collect::<BTreeSet<_>>();
            for obj in to_delete {
                let mut lock = cleaner_objects.lock().unwrap();
                lock.remove(&obj);
            }
        });

        cleaner_thread.join().unwrap();
        writer_thread.join().unwrap();

        let lock = objects.lock().unwrap();
        assert_eq!(*lock, BTreeSet::<i32>::from_iter([value_to_write, 2, 3]));
    });
}

#[rstest]
#[case::write_new(4)]
#[case::write_existing(1)]
fn simulate_proposed_lock_per_object_clean_behavior(
    #[case] value_to_write: i32,
    #[values(true, false)] tags_first: bool,
) {
    loom::model(move || {
        // Start out with `1` already as garbage.
        let tags = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([2, 3])));
        let objects = Arc::new(Mutex::new(BTreeSet::<i32>::from_iter([1, 2, 3])));
        let staged = Arc::new(Mutex::new(BTreeSet::<i32>::new()));
        // Because we only ever simulate writing one object, we can use this
        // single mutex as a stand-in for a per-object lock map. This avoids
        // clouding the test with having to wrap a map of locks with another
        // lock.
        let per_object_lock = Arc::new(Mutex::new(()));

        let writer_per_object_lock = Arc::clone(&per_object_lock);
        let writer_tags = Arc::clone(&tags);
        let writer_objects = Arc::clone(&objects);
        let writer_staged = Arc::clone(&staged);
        let writer_thread = thread::spawn(move || {
            // Before writing anything, acquire the per-object lock. It needs to
            // be held up until the staging file and object are written.
            let _object_lock_guard = writer_per_object_lock.lock().unwrap();

            // Before writing any objects, a writer must create a hard reference
            // to them by "staging them":
            let mut lock = writer_staged.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            // Now it is safe to add to objects.
            let mut lock = writer_objects.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);

            drop(_object_lock_guard);

            let mut lock = writer_tags.lock().unwrap();
            lock.insert(value_to_write);
            drop(lock);
            // In this variation, the staging file is not safe to delete
            // immediately. Obviously if the staging file is never deleted, then
            // the object will never be deleted. In practice, the staging file
            // will only be enforced for a configurable period of time and then
            // it could be removed.
        });

        let cleaner_per_object_lock = Arc::clone(&per_object_lock);
        let cleaner_tags = Arc::clone(&tags);
        let cleaner_objects = Arc::clone(&objects);
        let cleaner_staged = Arc::clone(&staged);
        let cleaner_thread = thread::spawn(move || {
            // This version doesn't have a global lock while deleting objects.
            // But it uses a lock per object to synchronize with writers.

            // Simulate reading tags and objects in either order.
            // Running two test variants is vastly faster than using threads to
            // simulate the order being non-deterministic.
            let (objects_snapshot, tags_snapshot) = if tags_first {
                let lock = cleaner_tags.lock().unwrap();
                let _tags_snapshot = (*lock).clone();
                drop(lock);
                let lock = cleaner_objects.lock().unwrap();
                let _objects_snapshot = (*lock).clone();
                drop(lock);
                (_objects_snapshot, _tags_snapshot)
            } else {
                let lock = cleaner_objects.lock().unwrap();
                let _objects_snapshot = (*lock).clone();
                drop(lock);
                let lock = cleaner_tags.lock().unwrap();
                let _tags_snapshot = (*lock).clone();
                drop(lock);
                (_objects_snapshot, _tags_snapshot)
            };

            let to_delete = objects_snapshot
                .difference(&tags_snapshot)
                .copied()
                .collect::<BTreeSet<_>>();

            for obj in to_delete {
                // Acquire the per-object lock to check if it is currently
                // staged. In this test, this condition is only possible when
                // `obj` == `value_to_write`.
                // This lock must be held through to the deletion to stop
                // writers from staging the object while we delete it.
                let _guard = if obj == value_to_write {
                    let _object_lock_guard = cleaner_per_object_lock.lock().unwrap();
                    let staged_lock = cleaner_staged.lock().unwrap();
                    if staged_lock.contains(&obj) {
                        // Still staged, can't delete.
                        //
                        // Per #1282, if any individual object is found to be
                        // ineligible for deletion, then any deletions that have
                        // already taken place may be a child of this object and
                        // now the repo is corrupted.
                        //
                        // This could perhaps be solved by doing deletions in
                        // two passes so they can be rolled back if needed.
                        continue;
                    }
                    Some(_object_lock_guard)
                } else {
                    None
                };

                let mut lock = cleaner_objects.lock().unwrap();
                lock.remove(&obj);
            }
        });

        cleaner_thread.join().unwrap();
        writer_thread.join().unwrap();

        let lock = objects.lock().unwrap();
        assert_eq!(*lock, BTreeSet::<i32>::from_iter([value_to_write, 2, 3]));
    });
}
fn main() {
    println!("Hello, world!");
}
