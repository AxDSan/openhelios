// SPDX-License-Identifier: GPL-2.0-or-later
//
// acer_gaming_probe - expose the Acer "gaming function" WMI method to userspace.
//
// On Acer Predator/Nitro laptops the ACPI firmware exposes a WMI device
// (UID "APGe", class AcerGamingFunction) whose _WDG maps the GUID
//   7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56
// to the ACPI control method WMBH.  WMBH(Arg1, Arg2) is a dispatcher:
//
//     Arg1 = "method id" (sub-command selector)
//     Arg2 = input byte buffer
//
// and it internally does `MTID = Arg1; WMIB = Arg2; WSSP = 0xD0` -- i.e. it
// copies the buffer into a shared-memory region and pokes an SMI/EC mailbox,
// then returns a response buffer.
//
// This module deliberately does NOT claim the WMI device and does NOT replace
// acer-wmi; it just calls the exported wmi_evaluate_method() by GUID so we can
// probe the protocol from userspace:
//
//   write() : byte 0 = method id, bytes 1..N = raw input buffer
//   read()  : returns the raw WMI response of the last call
//
// This is a research/probing tool.  Once the exact command for a given light
// is known, it can be replaced by a tiny purpose-built module or by the
// existing community "facer" module.

#include <linux/module.h>
#include <linux/miscdevice.h>
#include <linux/fs.h>
#include <linux/uaccess.h>
#include <linux/acpi.h>
#include <linux/slab.h>
#include <linux/mutex.h>

#define AGP_GUID	"7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56"
#define AGP_MAXBUF	1024

static u8	agp_resp[AGP_MAXBUF];
static size_t	agp_resp_len;
static u32	agp_last_status;
static DEFINE_MUTEX(agp_lock);

static ssize_t agp_write(struct file *filp, const char __user *ubuf,
			 size_t count, loff_t *ppos)
{
	u8 kbuf[AGP_MAXBUF];
	struct acpi_buffer input;
	struct acpi_buffer output = { ACPI_ALLOCATE_BUFFER, NULL };
	union acpi_object *obj;
	acpi_status status;
	u32 method_id;

	if (count < 1 || count > AGP_MAXBUF)
		return -EINVAL;
	if (copy_from_user(kbuf, ubuf, count))
		return -EFAULT;

	method_id = kbuf[0];
	input.length  = count - 1;
	input.pointer = kbuf + 1;

	mutex_lock(&agp_lock);

	status = wmi_evaluate_method(AGP_GUID, 0, method_id, &input, &output);

	agp_resp_len = 0;
	agp_last_status = status;

	if (ACPI_SUCCESS(status) && output.pointer) {
		obj = output.pointer;
		if (obj->type == ACPI_TYPE_BUFFER) {
			agp_resp_len = min_t(size_t, obj->buffer.length,
					     AGP_MAXBUF);
			memcpy(agp_resp, obj->buffer.pointer, agp_resp_len);
		} else if (obj->type == ACPI_TYPE_INTEGER) {
			agp_resp[0] = (u8)obj->integer.value;
			agp_resp_len = 1;
		}
	}

	mutex_unlock(&agp_lock);
	kfree(output.pointer);

	if (ACPI_FAILURE(status))
		return -EIO;

	return count;
}

static ssize_t agp_read(struct file *filp, char __user *ubuf,
			size_t count, loff_t *ppos)
{
	ssize_t ret;

	mutex_lock(&agp_lock);
	if (*ppos >= agp_resp_len) {
		mutex_unlock(&agp_lock);
		return 0;
	}
	if (count > agp_resp_len - *ppos)
		count = agp_resp_len - *ppos;
	if (copy_to_user(ubuf, agp_resp + *ppos, count)) {
		mutex_unlock(&agp_lock);
		return -EFAULT;
	}
	*ppos += count;
	ret = count;
	mutex_unlock(&agp_lock);

	return ret;
}

static const struct file_operations agp_fops = {
	.owner = THIS_MODULE,
	.write = agp_write,
	.read  = agp_read,
	.llseek = default_llseek,
};

static struct miscdevice agp_dev = {
	.minor = MISC_DYNAMIC_MINOR,
	.name  = "acer_gaming_wmi",
	.fops  = &agp_fops,
	.mode  = 0666,
};

static int __init agp_init(void)
{
	int ret;

	ret = misc_register(&agp_dev);
	if (ret)
		return ret;

	pr_info("acer_gaming_probe: /dev/%s ready (GUID %s)\n",
		agp_dev.name, AGP_GUID);
	return 0;
}

static void __exit agp_exit(void)
{
	misc_deregister(&agp_dev);
	pr_info("acer_gaming_probe: unloaded\n");
}

module_init(agp_init);
module_exit(agp_exit);

MODULE_AUTHOR("reverse-engineering session");
MODULE_DESCRIPTION("Expose Acer gaming-function WMI method to userspace (probing)");
MODULE_LICENSE("GPL");
MODULE_ALIAS("wmi:" AGP_GUID);
