use std::collections::BTreeSet;

use loom::sync::{Arc, Mutex};
use loom::thread;

#[test]
fn simulate_existing_clean_behavior() {
    loom::model(|| {
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
            lock.insert(4);
            drop(lock);
            let mut lock = writer_tags.lock().unwrap();
            lock.insert(4);
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
        assert_eq!(*lock, BTreeSet::<i32>::from_iter([2, 3, 4]));
    });
}

#[test]
fn simulate_proposed_clean_behavior() {
    loom::model(|| {
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
            lock.insert(4);
            drop(lock);
            // Now it is safe to add to objects.
            let mut lock = writer_objects.lock().unwrap();
            lock.insert(4);
            drop(lock);
            let mut lock = writer_tags.lock().unwrap();
            lock.insert(4);
            drop(lock);
            // With the tag written, it is now safe to unstage the object.
            let mut lock = writer_staged.lock().unwrap();
            lock.remove(&4);
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
        assert_eq!(*lock, BTreeSet::<i32>::from_iter([2, 3, 4]));
    });
}

#[test]
fn simulate_proposed_clean_behavior_version_2() {
    loom::model(|| {
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
            lock.insert(4);
            drop(lock);
            // Now it is safe to add to objects.
            let mut lock = writer_objects.lock().unwrap();
            lock.insert(4);
            drop(lock);
            let mut lock = writer_tags.lock().unwrap();
            lock.insert(4);
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
        assert_eq!(*lock, BTreeSet::<i32>::from_iter([2, 3, 4]));
    });
}

fn main() {
    println!("Hello, world!");
}
